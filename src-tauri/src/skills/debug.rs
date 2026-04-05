/// Debug skill — two-phase investigation and fix.
///
/// Phase 1 (Diagnose): Claude analyses the codebase read-only to identify the
///   root cause and describe the minimal fix.
/// Phase 2 (Fix): Claude applies the fix based on the diagnosis.
///
/// When the Agent SDK sidecar is configured, both phases run through the SDK
/// (one read-only query, one with write permissions). Otherwise falls back to
/// the legacy CLI runner mode (claude + codex).

use crate::config::AppConfig;
use crate::prompts::Prompts;
use crate::sidecar::{self, SidecarState};
use super::{runners, BlackboardEvent};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    config:       &AppConfig,
    prompts:      &Prompts,
    sidecar:      &SidecarState,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    if config.agent.is_configured() {
        run_via_sidecar(task, workspace, context, config, prompts, sidecar, window_label, app_handle, token).await
    } else {
        run_via_cli(task, workspace, context, prompts, window_label, app_handle, token).await
    }
}

// ── Agent SDK path ───────────────────────────────────────────────────────────

async fn run_via_sidecar(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    config:       &AppConfig,
    prompts:      &Prompts,
    sidecar:      &SidecarState,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    // ── Phase 1: Diagnose (read-only) ────────────────────────────────────────
    emit_debug_event(app_handle, window_label, "diagnosing",
        "Claude is analysing the codebase to diagnose the issue.".to_string())?;

    let diagnose_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_claude, &[("issue", task)]),
    );

    let diagnosis = sidecar::run_agent_query(
        sidecar, &config.agent,
        &diagnose_prompt, workspace,
        "plan",                                     // read-only mode
        &["Read", "Grep", "Glob"],                  // no write tools
        window_label, app_handle, token.clone(),
    ).await?;

    emit_debug_event(app_handle, window_label, "diagnosed",
        "Claude completed root-cause analysis. Now applying the fix.".to_string())?;

    // ── Phase 2: Fix (with write permissions) ────────────────────────────────
    emit_debug_event(app_handle, window_label, "fixing",
        "Claude is applying the fix based on the diagnosis.".to_string())?;

    let fix_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_codex, &[("issue", task)]),
    );
    let full_prompt = format!(
        "{fix_prompt}\n\n## Diagnosis (Phase 1)\n\n{diagnosis}\n\n\
         Apply the fix described above. Do not deviate from the diagnosis unless \
         you find a clear error in the analysis."
    );

    sidecar::run_agent_query(
        sidecar, &config.agent,
        &full_prompt, workspace,
        "acceptEdits",                              // write mode
        &["Read", "Edit", "Write", "Glob", "Grep", "Bash"],
        window_label, app_handle, token,
    ).await?;

    emit_debug_event(app_handle, window_label, "complete",
        "Debug skill finished — diagnosis and fix applied.".to_string())?;

    Ok(())
}

// ── Legacy CLI fallback ──────────────────────────────────────────────────────

async fn run_via_cli(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    emit_debug_event(app_handle, window_label, "diagnosing",
        "Claude is analysing the codebase to diagnose the issue.".to_string())?;

    let diagnose_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_claude, &[("issue", task)]),
    );
    let diagnosis = runners::claude(&diagnose_prompt, workspace, window_label, app_handle, token.clone()).await?;

    emit_debug_event(app_handle, window_label, "diagnosed",
        "Claude completed root-cause analysis. Codex will now apply the fix.".to_string())?;

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
