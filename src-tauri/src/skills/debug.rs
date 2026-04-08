use super::{emit_skill_event, record_skill_evidence};
/// Debug skill — two-phase investigation and fix.
///
/// Phase 1 (Diagnose): Agent analyses the codebase read-only to identify the
///   root cause and describe the minimal fix.
/// Phase 2 (Fix): Agent applies the fix based on the diagnosis.
///
/// All execution goes through the tool_use agent loop (direct API calls,
/// no CLI needed).
use crate::config::AppConfig;
use crate::prompts::Prompts;
use crate::tool_runner;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task: &str,
    workspace: Option<&str>,
    context: Option<&str>,
    config: &AppConfig,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let debug_artifacts = || vec![".ai-dev-hub/bugs.md".to_string(), ".ai-dev-hub/change.log".to_string()];

    // ── Phase 1: Diagnose (read-only) ────────────────────────────────────────
    emit_skill_event(
        app_handle,
        window_label,
        "diagnosing",
        "Agent is analysing the codebase to diagnose the issue.".to_string(),
    )?;
    record_skill_evidence(
        workspace,
        "debug_started",
        &format!("Debug started for issue: {task}"),
        "system",
        debug_artifacts(),
    );

    let diagnose_prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_claude, &[("issue", task)]),
    );

    let diagnosis = tool_runner::run_read_only(
        config,
        "You are a senior developer performing root-cause analysis. \
         Read the relevant source files, trace the code path, and identify the bug. \
         This is a read-only diagnosis phase — only view, grep, and glob tools are available.",
        &diagnose_prompt,
        workspace,
        window_label,
        app_handle,
        token.clone(),
    )
    .await?;

    emit_skill_event(
        app_handle,
        window_label,
        "diagnosed",
        "Agent completed root-cause analysis. Now applying the fix.".to_string(),
    )?;
    record_skill_evidence(
        workspace,
        "debug_diagnosed",
        "Agent completed root-cause analysis.",
        "agent",
        debug_artifacts(),
    );

    // ── Phase 2: Fix (with write permissions) ────────────────────────────────
    emit_skill_event(
        app_handle,
        window_label,
        "fixing",
        "Agent is applying the fix based on the diagnosis.".to_string(),
    )?;

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
        window_label,
        app_handle,
        token,
    )
    .await?;

    emit_skill_event(
        app_handle,
        window_label,
        "complete",
        "Debug skill finished — diagnosis and fix applied.".to_string(),
    )?;
    record_skill_evidence(
        workspace,
        "debug_completed",
        "Debug skill finished — diagnosis and fix applied.",
        "agent",
        debug_artifacts(),
    );

    Ok(())
}
