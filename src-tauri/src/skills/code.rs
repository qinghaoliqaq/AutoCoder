/// Code skill - per-subtask implementation loop driven by a shared blackboard.

use crate::prompts::Prompts;
use super::{
    blackboard::{
        change_log_entries, extract_paths, relative_paths, tick_plan_checkbox, Blackboard,
        SubtaskCard, BLACKBOARD_MD,
    },
    runners,
    vendored::{load as load_vendored_skill, select_for_subtask, VendoredSkill},
    BlackboardEvent, ToolLog,
};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

const MAX_SUBTASK_ATTEMPTS: u32 = 3;

pub(super) async fn run(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    let workspace = workspace.ok_or("Code mode requires an existing workspace from plan mode")?;
    let mut board = Blackboard::load_or_create(workspace, task)?;
    board.persist(workspace)?;

    emit_blackboard(
        app_handle,
        window_label,
        None,
        "initialized",
        format!(
            "Shared blackboard initialized. {} subtasks loaded from PLAN.md.",
            board.subtasks.len()
        ),
    )?;

    let base_prompt = Prompts::render(&prompts.code_claude, &[("task", task)]);
    let ordered_subtasks = board.pending_subtasks();
    let total = board.subtasks.len();

    for (index, card) in ordered_subtasks.iter().enumerate() {
        run_subtask(
            index + 1,
            total,
            card,
            task,
            workspace,
            context,
            &base_prompt,
            &mut board,
            window_label,
            app_handle,
            token.clone(),
        )
        .await?;
    }

    board.complete_if_finished();
    board.persist(workspace)?;
    emit_blackboard(
        app_handle,
        window_label,
        None,
        "completed",
        "All planned subtasks passed inline review and were recorded on the shared blackboard."
            .to_string(),
    )?;
    Ok(())
}

async fn run_subtask(
    ordinal:      usize,
    total:        usize,
    initial_card: &SubtaskCard,
    task:         &str,
    workspace:    &str,
    context:      Option<&str>,
    base_prompt:  &str,
    board:        &mut Blackboard,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    let subtask_id = initial_card.id.clone();

    loop {
        let attempt = board.begin_attempt(&subtask_id)?;
        board.persist(workspace)?;

        let card = board.subtask(&subtask_id)?.clone();
        let summary = if attempt == 1 {
            format!(
                "Subtask {ordinal}/{total}: {}. {} is now implementing {} via the shared blackboard.",
                card.id, "Claude", card.title
            )
        } else {
            format!(
                "Subtask {ordinal}/{total}: {} needs another pass. Claude is fixing {} using Codex findings from {}.",
                card.id, card.title, BLACKBOARD_MD
            )
        };
        emit_blackboard(
            app_handle,
            window_label,
            Some(card.id.clone()),
            "subtask_started",
            summary,
        )?;

        let before_changes = change_log_entries(workspace);
        let vendored_skill = select_for_subtask(&card)
            .map(|skill_id| load_vendored_skill(skill_id, app_handle))
            .transpose()?;
        if let Some(skill) = &vendored_skill {
            emit_vendored_skill_log(app_handle, window_label, "claude", skill, &card)?;
            emit_blackboard(
                app_handle,
                window_label,
                Some(card.id.clone()),
                "vendored_skill_selected",
                format!(
                    "Subtask {} is using packaged helper skill {}.",
                    card.id,
                    skill.id.label()
                ),
            )?;
        }
        let claude_prompt = if card.review_findings.is_empty() {
            build_implement_prompt(base_prompt, task, &card, vendored_skill.as_ref())
        } else {
            build_fix_prompt(base_prompt, task, &card, vendored_skill.as_ref())
        };
        let claude_prompt = super::inject_context(context, claude_prompt);
        let claude_output = runners::claude(
            &claude_prompt,
            Some(workspace),
            window_label,
            app_handle,
            token.clone(),
        )
        .await?;

        let after_changes = change_log_entries(workspace);
        let new_changes = delta_entries(&before_changes, &after_changes);
        let files_touched = relative_paths(workspace, &extract_paths(&new_changes));
        let implementation = parse_implementation_report(&claude_output, &files_touched, &card.id);
        board.record_implementation(&card.id, implementation.summary, implementation.files_touched)?;
        board.persist(workspace)?;

        emit_blackboard(
            app_handle,
            window_label,
            Some(card.id.clone()),
            "implemented",
            format!(
                "Claude finished {} attempt {}. Codex is now reviewing the subtask through the blackboard.",
                card.id, attempt
            ),
        )?;

        let review_card = board.subtask(&card.id)?.clone();
        let review_prompt = super::inject_context(context, build_review_prompt(task, &review_card));
        let review_output = runners::codex(
            &review_prompt,
            Some(workspace),
            window_label,
            app_handle,
            token.clone(),
        )
        .await?;
        let review = parse_review_report(&review_output);
        board.record_review(
            &card.id,
            review.passed,
            review.summary.clone(),
            review.findings.clone(),
        )?;

        if review.passed {
            tick_plan_checkbox(workspace, &card.id)?;
            board.complete_if_finished();
            board.persist(workspace)?;
            emit_blackboard(
                app_handle,
                window_label,
                Some(card.id.clone()),
                "passed",
                format!(
                    "Subtask {} passed Codex review and was checked off in PLAN.md.",
                    card.id
                ),
            )?;
            return Ok(());
        }

        if attempt >= MAX_SUBTASK_ATTEMPTS {
            let reason = format!(
                "Subtask {} failed inline review after {} attempts: {}",
                card.id, MAX_SUBTASK_ATTEMPTS, review.summary
            );
            board.mark_failed(&card.id, reason.clone())?;
            board.persist(workspace)?;
            emit_blackboard(
                app_handle,
                window_label,
                Some(card.id.clone()),
                "failed",
                reason.clone(),
            )?;
            return Err(reason);
        }

        board.persist(workspace)?;
        emit_blackboard(
            app_handle,
            window_label,
            Some(card.id.clone()),
            "needs_fix",
            format!(
                "Codex rejected {} on attempt {}. Claude will retry using the shared blackboard findings.",
                card.id, attempt
            ),
        )?;
    }
}

fn build_implement_prompt(
    base_prompt: &str,
    task: &str,
    card: &SubtaskCard,
    vendored_skill: Option<&VendoredSkill>,
) -> String {
    format!(
        "{base_prompt}\n\n\
Shared-blackboard contract:\n\
- Read PLAN.md and {BLACKBOARD_MD} in the current directory before making changes.\n\
- Do not rely on direct agent-to-agent conversation.\n\
- Do not implement the whole project in one pass; focus only on the current subtask.\n\
- You may touch shared code if required, but only to complete this subtask cleanly.\n\
- If packaged vendored skill guidance conflicts with PLAN.md, PLAN_BLACKBOARD.md, {BLACKBOARD_MD}, or the current subtask contract, follow the local project rules.\n\
\n\
Current task context: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
\n\
{vendored_block}\n\
\n\
Completion rule:\n\
- Finish only when this subtask is fully implemented and ready for Codex review.\n\
- Keep your response concise.\n\
\n\
At the very end output exactly these lines:\n\
SUBTASK_ID: {id}\n\
IMPLEMENTATION_SUMMARY: <one concise paragraph>\n\
FILES_TOUCHED: <comma-separated relative paths or none>",
        id = card.id,
        title = card.title,
        description = card.description,
        vendored_block = render_vendored_block(vendored_skill),
    )
}

fn build_fix_prompt(
    base_prompt: &str,
    task: &str,
    card: &SubtaskCard,
    vendored_skill: Option<&VendoredSkill>,
) -> String {
    let findings = if card.review_findings.is_empty() {
        "- none".to_string()
    } else {
        card.review_findings
            .iter()
            .map(|finding| format!("- {finding}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "{base_prompt}\n\n\
Shared-blackboard contract:\n\
- Read PLAN.md and {BLACKBOARD_MD} in the current directory before making changes.\n\
- Treat the review findings below as the only coordination channel from Codex.\n\
- Fix the current subtask; do not drift into unrelated features.\n\
- If packaged vendored skill guidance conflicts with PLAN.md, PLAN_BLACKBOARD.md, {BLACKBOARD_MD}, or the current subtask contract, follow the local project rules.\n\
\n\
Current task context: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
\n\
{vendored_block}\n\
\n\
Blackboard review findings to resolve:\n\
{findings}\n\
\n\
At the very end output exactly these lines:\n\
SUBTASK_ID: {id}\n\
IMPLEMENTATION_SUMMARY: <one concise paragraph about the fixes>\n\
FILES_TOUCHED: <comma-separated relative paths or none>",
        id = card.id,
        title = card.title,
        description = card.description,
        vendored_block = render_vendored_block(vendored_skill),
    )
}

fn build_review_prompt(task: &str, card: &SubtaskCard) -> String {
    let files = if card.files_touched.is_empty() {
        "none".to_string()
    } else {
        card.files_touched.join(", ")
    };

    format!(
        "You are Codex reviewing exactly one implementation subtask.\n\n\
Shared-blackboard contract:\n\
- Read PLAN.md and {BLACKBOARD_MD} from the current directory before reviewing.\n\
- Do not rely on direct Claude transcript as the source of truth.\n\
- Do not edit files. Your job is review only.\n\
- Review only the current subtask, but verify integration points it depends on.\n\
\n\
Overall task: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
- Files recently touched: {files}\n\
- Blackboard implementation summary: {implementation}\n\
\n\
Review standard:\n\
- PASS only if this subtask is implemented, wired correctly, and has no obvious correctness gap in scope.\n\
- FAIL if required behavior is missing, incorrect, fragile, or not integrated.\n\
\n\
At the very end output exactly this shape:\n\
REVIEW_DECISION: PASS or FAIL\n\
REVIEW_SUMMARY: <one concise paragraph>\n\
REVIEW_FINDINGS:\n\
- <specific actionable issue>\n\
\n\
If there are no issues, write:\n\
REVIEW_FINDINGS:\n\
- none",
        id = card.id,
        title = card.title,
        description = card.description,
        implementation = card.latest_implementation.as_deref().unwrap_or("none"),
    )
}

fn emit_blackboard(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    subtask_id: Option<String>,
    status: &str,
    summary: String,
) -> Result<(), String> {
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "blackboard-updated",
            BlackboardEvent {
                subtask_id,
                status: status.to_string(),
                summary,
            },
        )
        .map_err(|e| format!("Emit error: {e}"))
}

fn emit_vendored_skill_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    agent: &str,
    skill: &VendoredSkill,
    card: &SubtaskCard,
) -> Result<(), String> {
    let ts = chrono::Utc::now().timestamp_millis() as u64;
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "tool-log",
            ToolLog {
                agent: agent.to_string(),
                tool: "VendoredSkill".to_string(),
                input: format!("{} -> {} {}", skill.id.slug(), card.id, card.title),
                timestamp: ts,
            },
        )
        .map_err(|e| format!("Emit error: {e}"))
}

fn render_vendored_block(vendored_skill: Option<&VendoredSkill>) -> String {
    let Some(skill) = vendored_skill else {
        return "Packaged vendored skill: none for this subtask.".to_string();
    };

    format!(
        "Packaged vendored skill available:\n\
- ID: {id}\n\
- Skill file: {skill_path}\n\
- Skill root: {root_dir}\n\
- Read the full vendored skill file before implementing if you need its detailed workflow.\n\
- You may also read any references under the skill root if they help with this specific subtask.\n\
\n\
Vendored skill excerpt:\n\
```markdown\n\
{excerpt}\n\
```",
        id = skill.id.slug(),
        skill_path = skill.skill_path.display(),
        root_dir = skill.root_dir.display(),
        excerpt = skill.excerpt
    )
}

fn delta_entries(before: &[String], after: &[String]) -> Vec<String> {
    if after.len() >= before.len() {
        after[before.len()..].to_vec()
    } else {
        after.to_vec()
    }
}

struct ImplementationReport {
    summary: String,
    files_touched: Vec<String>,
}

fn parse_implementation_report(
    output: &str,
    observed_files: &[String],
    subtask_id: &str,
) -> ImplementationReport {
    let summary = extract_marker_line(output, "IMPLEMENTATION_SUMMARY:")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_summary(output, &format!("Implementation finished for {subtask_id}")));

    let marker_files = extract_marker_line(output, "FILES_TOUCHED:")
        .map(|line| split_csv(&line))
        .unwrap_or_default();

    let files_touched = if !marker_files.is_empty() && marker_files != ["none".to_string()] {
        marker_files
    } else {
        observed_files.to_vec()
    };

    ImplementationReport { summary, files_touched }
}

struct ReviewReport {
    passed: bool,
    summary: String,
    findings: Vec<String>,
}

fn parse_review_report(output: &str) -> ReviewReport {
    let decision = extract_marker_line(output, "REVIEW_DECISION:")
        .unwrap_or_else(|| "FAIL".to_string())
        .to_uppercase();
    let passed = decision.contains("PASS");
    let summary = extract_marker_line(output, "REVIEW_SUMMARY:")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_summary(output, "Codex review completed"));

    let findings = extract_list_after_header(output, "REVIEW_FINDINGS:");
    let findings = if findings.len() == 1 && findings[0].eq_ignore_ascii_case("none") {
        Vec::new()
    } else {
        findings
    };

    ReviewReport {
        passed,
        summary,
        findings,
    }
}

fn extract_marker_line(output: &str, prefix: &str) -> Option<String> {
    output
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(prefix).map(|s| s.trim().to_string()))
}

fn extract_list_after_header(output: &str, header: &str) -> Vec<String> {
    let mut collecting = false;
    let mut items = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            collecting = true;
            continue;
        }
        if collecting {
            if let Some(item) = trimmed.strip_prefix("- ") {
                items.push(item.trim().to_string());
                continue;
            }
            if !items.is_empty() && !trimmed.is_empty() {
                break;
            }
        }
    }

    items
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn fallback_summary(output: &str, fallback: &str) -> String {
    let compact = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| {
            !line.starts_with("SUBTASK_ID:")
                && !line.starts_with("IMPLEMENTATION_SUMMARY:")
                && !line.starts_with("FILES_TOUCHED:")
                && !line.starts_with("REVIEW_DECISION:")
                && !line.starts_with("REVIEW_SUMMARY:")
                && !line.starts_with("REVIEW_FINDINGS:")
                && !line.starts_with("- ")
        })
        .map(|line| line.chars().take(240).collect::<String>());

    compact.unwrap_or_else(|| fallback.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_implementation_report_prefers_markers() {
        let report = parse_implementation_report(
            "note\nIMPLEMENTATION_SUMMARY: added auth flow\nFILES_TOUCHED: src/app.ts, src/api.ts",
            &[],
            "F1",
        );
        assert_eq!(report.summary, "added auth flow");
        assert_eq!(report.files_touched, vec!["src/app.ts", "src/api.ts"]);
    }

    #[test]
    fn parse_review_report_reads_findings() {
        let report = parse_review_report(
            "REVIEW_DECISION: FAIL\nREVIEW_SUMMARY: login screen missing loading state\nREVIEW_FINDINGS:\n- add disabled submit state\n- render API error message\n",
        );
        assert!(!report.passed);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn delta_entries_returns_tail() {
        let before = vec!["a".to_string()];
        let after = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(delta_entries(&before, &after), vec!["b".to_string(), "c".to_string()]);
    }
}
