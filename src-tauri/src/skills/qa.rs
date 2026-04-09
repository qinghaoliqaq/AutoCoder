use super::{QaResult, ToolLog};
use super::evidence::{self, read_evidence_index, EvidenceEvent, EVIDENCE_INDEX_JSON};
use super::planning_schema::{read_plan_acceptance_lenient, PLAN_ACCEPTANCE_JSON};
use crate::{config::AppConfig, prompts::Prompts, tool_runner};
use chrono::Utc;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task: &str,
    issue: Option<&str>,
    workspace: Option<&str>,
    context: Option<&str>,
    config: &AppConfig,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let evidence_section = if let Some(workspace) = workspace {
        match load_evidence_section(workspace) {
            Ok(section) => section,
            Err(err) => Some(format!(
                "## Evidence Index Warning\n\n{EVIDENCE_INDEX_JSON} could not be refreshed or read.\n\nReason: {err}"
            )),
        }
    } else {
        None
    };
    let metrics_section = workspace.and_then(|ws| {
        evidence::compute_evidence_metrics(ws).map(|m| evidence::format_metrics_section(&m))
    });
    let (acceptance, acceptance_warning) = workspace
        .map(read_plan_acceptance_lenient)
        .unwrap_or((None, None));
    let acceptance_section = acceptance.map(|acceptance| {
        let json = serde_json::to_string_pretty(&acceptance).unwrap_or_else(|_| "{}".to_string());
        format!("## Structured Acceptance ({PLAN_ACCEPTANCE_JSON})\n\n```json\n{json}\n```")
    });
    if let Some(warning) = &acceptance_warning {
        emit_acceptance_warning_log(app_handle, window_label, warning)?;
    }
    let warning_section = acceptance_warning.map(|warning| {
        format!(
            "## Structured Acceptance Warning\n\n{PLAN_ACCEPTANCE_JSON} could not be used. Continue with fallback review criteria only.\n\nReason: {warning}"
        )
    });
    let merged_context = super::merge_context_sections(&[
        context.map(ToOwned::to_owned),
        metrics_section,
        evidence_section,
        warning_section,
        acceptance_section,
    ]);
    let prompt = super::inject_context(
        merged_context.as_deref(),
        Prompts::render(
            &prompts.qa_claude,
            &[("task", task), ("issue", issue.unwrap_or("none"))],
        ),
    );

    let output = tool_runner::run_read_only(
        config,
        "You are a senior QA engineer performing acceptance testing. \
         Read source files, check tests, review evidence, and assess project quality. \
         This is a read-only review — only view, grep, and glob tools are available.",
        &prompt,
        workspace,
        window_label,
        app_handle,
        token,
    )
    .await?;
    let health_score = workspace
        .and_then(|ws| evidence::compute_evidence_metrics(ws))
        .map(|m| m.health_score)
        .unwrap_or(0);
    let result = parse_qa_result(&output, health_score)?;
    if let Some(workspace) = workspace {
        // Evidence recording is best-effort — must never fail the QA skill.
        let _ = evidence::record_event(
            workspace,
            EvidenceEvent {
                ts: Utc::now().timestamp_millis() as u64,
                event_type: match result.verdict.as_str() {
                    "PASS" => "qa_passed".to_string(),
                    "PASS_WITH_CONCERNS" => "qa_pass_with_concerns".to_string(),
                    _ => "qa_failed".to_string(),
                },
                agent: "claude".to_string(),
                subtask_id: None,
                summary: format!(
                    "{} [confidence={}, health={}]",
                    result.summary, result.confidence_score, result.health_score
                ),
                artifacts: vec![
                    EVIDENCE_INDEX_JSON.to_string(),
                    "PROJECT_REPORT.md".to_string(),
                    "bugs.md".to_string(),
                ],
            },
        );
    }

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "qa-result",
            result,
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn load_evidence_section(workspace: &str) -> Result<Option<String>, String> {
    evidence::refresh_evidence_index(workspace)?;
    let Some(index) = read_evidence_index(workspace)? else {
        return Ok(None);
    };
    let json = serde_json::to_string_pretty(&index)
        .map_err(|e| format!("Cannot serialize {EVIDENCE_INDEX_JSON} for prompt: {e}"))?;

    // Include both the structured JSON and a human-readable digest summary.
    // The digest highlights trouble spots, failure patterns, and QA history
    // so the LLM can reason about multi-round quality trends.
    let digest = evidence::build_evidence_digest(workspace).unwrap_or_default();

    let mut section = format!("## Evidence Index ({EVIDENCE_INDEX_JSON})\n\n```json\n{json}\n```");
    if !digest.is_empty() {
        section.push_str("\n\n");
        section.push_str(&digest);
    }

    Ok(Some(section))
}

fn emit_acceptance_warning_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    warning: &str,
) -> Result<(), String> {
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "tool-log",
            ToolLog {
                agent: "system".to_string(),
                tool: "StructuredAcceptance".to_string(),
                input: format!("Fallback active: {warning}"),
                timestamp: Utc::now().timestamp_millis() as u64,
            },
        )
        .map_err(|e| e.to_string())
}

fn parse_qa_result(text: &str, health_score: u32) -> Result<QaResult, String> {
    let verdict = extract_marker_value(text, "QA_VERDICT")
        .ok_or_else(|| "QA output missing [QA_VERDICT:*] marker".to_string())?;
    let recommended_next_step = extract_marker_value(text, "QA_NEXT")
        .ok_or_else(|| "QA output missing [QA_NEXT:*] marker".to_string())?;
    let summary = extract_marker_value(text, "QA_SUMMARY")
        .ok_or_else(|| "QA output missing [QA_SUMMARY:*] marker".to_string())?;
    let issue = extract_marker_value(text, "QA_ISSUE")
        .ok_or_else(|| "QA output missing [QA_ISSUE:*] marker".to_string())?;
    let confidence_score = extract_marker_value(text, "QA_CONFIDENCE")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
        .min(100);

    if !matches!(verdict.as_str(), "PASS" | "PASS_WITH_CONCERNS" | "FAIL") {
        return Err(format!("Invalid QA verdict marker: {verdict}"));
    }
    if !matches!(
        recommended_next_step.as_str(),
        "complete" | "review" | "debug" | "code"
    ) {
        return Err(format!(
            "Invalid QA next-step marker: {recommended_next_step}"
        ));
    }
    validate_qa_transition(&verdict, &recommended_next_step)?;

    Ok(QaResult {
        verdict,
        recommended_next_step,
        summary,
        issue,
        confidence_score,
        health_score,
    })
}

fn validate_qa_transition(verdict: &str, recommended_next_step: &str) -> Result<(), String> {
    let valid = match verdict {
        "PASS" => recommended_next_step == "complete",
        "PASS_WITH_CONCERNS" => matches!(recommended_next_step, "complete" | "review"),
        "FAIL" => matches!(recommended_next_step, "review" | "debug" | "code"),
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        Err(format!(
            "Invalid QA verdict/next-step combination: {verdict} -> {recommended_next_step}"
        ))
    }
}

fn extract_marker_value(text: &str, name: &str) -> Option<String> {
    let prefix = format!("[{name}:");
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return Some(rest.trim_end_matches(']').trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_qa_result_reads_markers() {
        let text = "\
QA Verdict: PASS
\n\
[QA_VERDICT:PASS]
\n\
[QA_NEXT:complete]
\n\
[QA_SUMMARY:Feature is ready for handoff]
\n\
[QA_ISSUE:none]
\n\
[QA_CONFIDENCE:85]";
        let result = parse_qa_result(text, 90).unwrap();
        assert_eq!(result.verdict, "PASS");
        assert_eq!(result.recommended_next_step, "complete");
        assert_eq!(result.summary, "Feature is ready for handoff");
        assert_eq!(result.issue, "none");
        assert_eq!(result.confidence_score, 85);
        assert_eq!(result.health_score, 90);
    }

    #[test]
    fn parse_qa_result_defaults_confidence_when_missing() {
        let text = "\
[QA_VERDICT:PASS]
\n\
[QA_NEXT:complete]
\n\
[QA_SUMMARY:All good]
\n\
[QA_ISSUE:none]";
        let result = parse_qa_result(text, 75).unwrap();
        assert_eq!(result.confidence_score, 0);
        assert_eq!(result.health_score, 75);
    }

    #[test]
    fn parse_qa_result_clamps_confidence_to_100() {
        let text = "\
[QA_VERDICT:PASS]
\n\
[QA_NEXT:complete]
\n\
[QA_SUMMARY:All good]
\n\
[QA_ISSUE:none]
\n\
[QA_CONFIDENCE:150]";
        let result = parse_qa_result(text, 50).unwrap();
        assert_eq!(result.confidence_score, 100);
    }

    #[test]
    fn parse_qa_result_fails_closed_without_markers() {
        let err = parse_qa_result("plain text only", 0).unwrap_err();
        assert!(err.contains("QA_VERDICT"));
    }

    #[test]
    fn parse_qa_result_rejects_invalid_next_step() {
        let text = "\
[QA_VERDICT:FAIL]
\n\
[QA_NEXT:test]
\n\
[QA_SUMMARY:Need more evidence]
\n\
[QA_ISSUE:no test evidence]";
        let err = parse_qa_result(text, 0).unwrap_err();
        assert!(err.contains("Invalid QA next-step marker"));
    }

    #[test]
    fn parse_qa_result_rejects_invalid_pass_combination() {
        let text = "\
[QA_VERDICT:PASS]
\n\
[QA_NEXT:review]
\n\
[QA_SUMMARY:Looks good]
\n\
[QA_ISSUE:none]";
        let err = parse_qa_result(text, 0).unwrap_err();
        assert!(err.contains("Invalid QA verdict/next-step combination"));
    }

    #[test]
    fn parse_qa_result_rejects_invalid_pass_with_concerns_combination() {
        let text = "\
[QA_VERDICT:PASS_WITH_CONCERNS]
\n\
[QA_NEXT:code]
\n\
[QA_SUMMARY:Mostly usable]
\n\
[QA_ISSUE:minor gaps]";
        let err = parse_qa_result(text, 0).unwrap_err();
        assert!(err.contains("Invalid QA verdict/next-step combination"));
    }
}
