/// Debug skill — two-phase investigation and fix.
///
/// Phase 1 (Diagnose): Claude analyses the codebase read-only to identify the
///   root cause and describe the minimal fix.
/// Phase 2 (Fix): Codex applies the fix based on Claude's diagnosis.

use crate::prompts::Prompts;
use super::{runners, BlackboardEvent};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    // ── Phase 1: Claude diagnoses the issue (read-only) ──────────────────────
    emit_debug_event(app_handle, window_label, "diagnosing",
        "Claude is analysing the codebase to diagnose the issue.".to_string())?;

    let diagnose_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_claude, &[("issue", task)]),
    );
    let diagnosis = runners::claude(&diagnose_prompt, workspace, window_label, app_handle, token.clone()).await?;

    emit_debug_event(app_handle, window_label, "diagnosed",
        "Claude completed root-cause analysis. Codex will now apply the fix.".to_string())?;

    // ── Phase 2: Codex applies the fix informed by the diagnosis ─────────────
    emit_debug_event(app_handle, window_label, "fixing",
        "Codex is applying the fix based on Claude's diagnosis.".to_string())?;

    let fix_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_codex, &[("issue", task)]),
    );
    let full_prompt = format!(
        "{fix_prompt}\n\n## Diagnosis from Claude (Phase 1)\n\n{diagnosis}\n\n\
         Apply the fix described above. Do not deviate from the diagnosis unless \
         you find a clear error in the analysis."
    );
    runners::codex(&full_prompt, workspace, window_label, app_handle, token).await?;

    emit_debug_event(app_handle, window_label, "complete",
        "Debug skill finished — diagnosis and fix applied.".to_string())?;

    Ok(())
}

fn emit_debug_event(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    status: &str,
    summary: String,
) -> Result<(), String> {
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "blackboard-updated",
            BlackboardEvent {
                subtask_id: None,
                status: status.to_string(),
                summary,
            },
        )
        .map_err(|e| format!("Emit error: {e}"))
}
