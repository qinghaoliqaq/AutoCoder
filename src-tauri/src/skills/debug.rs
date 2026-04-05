/// Debug skill — two-phase investigation and fix.
///
/// Phase 1 (Diagnose): Claude analyses the codebase read-only to identify the
///   root cause and describe the minimal fix.
/// Phase 2 (Fix): Claude applies the fix based on the diagnosis.
///
/// When the Anthropic API is configured (via [agent] or [director] with
/// api_format = "anthropic"), both phases run through the tool_use agent loop
/// (direct API calls, no CLI needed). Otherwise falls back to the legacy CLI
/// runner mode (claude + codex).

use crate::config::AppConfig;
use crate::prompts::Prompts;
use crate::tool_runner;
use super::{runners, BlackboardEvent};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    config:       &AppConfig,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    if can_use_tool_runner(config) {
        run_via_api(task, workspace, context, config, prompts, window_label, app_handle, token).await
    } else {
        run_via_cli(task, workspace, context, prompts, window_label, app_handle, token).await
    }
}

/// Check if we can use the direct API tool_use loop.
fn can_use_tool_runner(config: &AppConfig) -> bool {
    // Option 1: [agent] section configured
    if config.agent.is_configured() {
        return true;
    }
    // Option 2: [director] is configured with Anthropic format
    config.is_configured()
        && config.director.api_format == crate::config::ApiFormat::Anthropic
}

// ── API tool_use path (no CLI needed) ────────────────────────────────────────

async fn run_via_api(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    config:       &AppConfig,
    prompts:      &Prompts,
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

    let diagnosis = tool_runner::run(
        config,
        "You are a senior developer performing root-cause analysis. \
         Read the relevant source files, trace the code path, and identify the bug. \
         Do NOT modify any files — this is a read-only diagnosis phase.",
        &diagnose_prompt,
        workspace,
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

    tool_runner::run(
        config,
        "You are a senior developer applying a precise bug fix. \
         Use the editor and bash tools to make the minimal correct fix. \
         Do not change unrelated code.",
        &full_prompt,
        workspace,
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
