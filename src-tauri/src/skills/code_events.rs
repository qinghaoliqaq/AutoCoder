/// Event emission helpers for the code skill.
///
/// Responsible for emitting Tauri events (blackboard-updated, tool-log)
/// and recording evidence entries.
use super::blackboard::{BLACKBOARD_JSON, BLACKBOARD_MD};
use super::vendored::VendoredSkill;
use super::{BlackboardEvent, ToolLog};
use super::evidence::{self, EvidenceEvent};
use crate::skills::blackboard::SubtaskCard;
use super::verifier::VERIFIER_RESULT_JSON;
use tauri::{Emitter, EventTarget};

pub(super) fn emit_blackboard(
    workspace: &str,
    app_handle: &tauri::AppHandle,
    window_label: &str,
    subtask_id: Option<String>,
    status: &str,
    summary: String,
) -> Result<(), String> {
    let event_subtask_id = subtask_id.clone();
    let event_summary = summary.clone();
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
        .map_err(|e| format!("Emit error: {e}"))?;
    evidence::record_event(
        workspace,
        EvidenceEvent {
            ts: chrono::Utc::now().timestamp_millis() as u64,
            event_type: status.to_string(),
            agent: evidence_agent_for_status(status).to_string(),
            subtask_id: event_subtask_id,
            summary: event_summary,
            artifacts: evidence_artifacts_for_status(status),
        },
    )
}

pub(super) fn emit_vendored_skill_log(
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
                tool: "BundledSkill".to_string(),
                input: format!("{} -> {} {}", skill.id.slug(), card.id, card.title),
                timestamp: ts,
            },
        )
        .map_err(|e| format!("Emit error: {e}"))
}

fn evidence_agent_for_status(status: &str) -> &'static str {
    match status {
        "subtask_started" | "implemented" => "claude",
        "verifier_passed" | "verifier_failed" => "verifier",
        "passed" | "needs_fix" => "codex",
        _ => "system",
    }
}

fn evidence_artifacts_for_status(status: &str) -> Vec<String> {
    match status {
        "verifier_passed" | "verifier_failed" => vec![
            BLACKBOARD_JSON.to_string(),
            BLACKBOARD_MD.to_string(),
            VERIFIER_RESULT_JSON.to_string(),
            ".ai-dev-hub/PLAN.md".to_string(),
        ],
        "subtask_started" | "implemented" | "passed" | "needs_fix" | "failed" => vec![
            BLACKBOARD_JSON.to_string(),
            BLACKBOARD_MD.to_string(),
            ".ai-dev-hub/PLAN.md".to_string(),
        ],
        _ => vec![BLACKBOARD_JSON.to_string(), BLACKBOARD_MD.to_string()],
    }
}
