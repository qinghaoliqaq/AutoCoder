/// Codex CLI runner — spawns `codex exec` in non-interactive JSON mode.
///
/// Parses the Codex JSON protocol:
///   "item.started" + type "command_execution" → tool call starting.
///   "item.completed" + type "agent_message"   → text reply.

use super::runner_lifecycle::{run_cli_process, LineAction};
use super::runner_workspace::{record_workspace_snapshot_diff, snapshot_workspace};
use super::{SkillChunk, ToolLog};
use serde_json::Value;
use tauri::{Emitter, EventTarget};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::runners::{
    build_codex_args, configured_execution_access_mode, resolve_cwd, CodexExecutionMode,
};

// ── Public wrappers ───────────────────────────────────────────────────

pub(crate) async fn codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt, cwd, window_label, app_handle, token,
        CodexExecutionMode::WorkspaceWrite, true, None,
    ).await
}

pub(crate) async fn codex_read_only(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt, cwd, window_label, app_handle, token,
        CodexExecutionMode::ReadOnlyReview, true, None,
    ).await
}

pub(crate) async fn codex_read_only_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt, cwd, window_label, app_handle, token,
        CodexExecutionMode::ReadOnlyReview, false, None,
    ).await
}

/// Like `codex_read_only_quiet`, but tags emitted `skill-chunk` events with a subtask ID.
pub(crate) async fn codex_read_only_quiet_subtask(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    subtask_id: &str,
) -> Result<String, String> {
    run_codex(
        prompt, cwd, window_label, app_handle, token,
        CodexExecutionMode::ReadOnlyReview, false, Some(subtask_id),
    ).await
}

// ── Core implementation ───────────────────────────────────────────────

async fn run_codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    mode: CodexExecutionMode,
    emit_chunks: bool,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    tracing::info!(mode = ?mode, cwd = ?cwd, "spawning codex");
    let mut cmd = Command::new("codex");
    let access_mode = configured_execution_access_mode();
    cmd.args(build_codex_args(prompt, mode, access_mode));
    let resolved_cwd = resolve_cwd(cwd);
    let workspace_before = if matches!(mode, CodexExecutionMode::WorkspaceWrite) && cwd.is_some() {
        Some(snapshot_workspace(&resolved_cwd))
    } else {
        None
    };
    cmd.current_dir(&resolved_cwd);

    let mut output = String::new();
    let owned_subtask_id = subtask_id.map(ToString::to_string);

    let process_result = run_cli_process(
        "codex",
        &mut cmd,
        window_label,
        token,
        |line| {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                return LineAction::Continue;
            };
            let ev_type = v["type"].as_str().unwrap_or("");

            if ev_type == "error" {
                let msg = v["message"].as_str()
                    .or_else(|| v["error"].as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                return LineAction::Error(format!("Codex error: {msg}"));
            }

            if let Some(item) = v.get("item") {
                handle_codex_item(
                    ev_type, item, emit_chunks, window_label, app_handle, &mut output,
                    owned_subtask_id.as_deref(),
                );
            }
            LineAction::Continue
        },
    ).await;

    // If we have output, a non-zero exit is acceptable.
    match process_result {
        Ok(()) => {}
        Err(e) if e.starts_with("codex exited with non-zero") && !output.is_empty() => {}
        Err(e) => return Err(e),
    }

    if let Some(before) = workspace_before {
        let after = snapshot_workspace(&resolved_cwd);
        record_workspace_snapshot_diff(&resolved_cwd, &before, &after);
    }

    Ok(output)
}

// ── Protocol handlers ────────────────────────────────────────────────

fn handle_codex_item(
    ev_type: &str,
    item: &Value,
    emit_chunks: bool,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    output: &mut String,
    subtask_id: Option<&str>,
) {
    match (ev_type, item["type"].as_str().unwrap_or("")) {
        ("item.started", "command_execution") => {
            let command = item["command"].as_str().unwrap_or("").to_string();
            if !command.is_empty() {
                let ts = chrono::Utc::now().timestamp_millis() as u64;
                let _ = app_handle.emit_to(
                    EventTarget::webview_window(window_label),
                    "tool-log",
                    ToolLog {
                        agent: "codex".to_string(),
                        tool: "Shell".to_string(),
                        input: command.chars().take(150).collect(),
                        timestamp: ts,
                    },
                );
            }
        }
        ("item.completed", "agent_message") => {
            if let Some(text) = item["text"].as_str() {
                let chunk = format!("{text}\n");
                output.push_str(&chunk);
                if emit_chunks {
                    let _ = app_handle.emit_to(
                        EventTarget::webview_window(window_label),
                        "skill-chunk",
                        SkillChunk {
                            agent: "codex".to_string(),
                            text: chunk,
                            reset: true,
                            subtask_id: subtask_id.map(ToString::to_string),
                        },
                    );
                }
            }
        }
        _ => {}
    }
}
