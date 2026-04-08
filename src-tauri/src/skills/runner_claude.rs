/// Claude CLI runner — spawns `claude -p` in stream-json mode.
///
/// Parses the Claude stream-json protocol:
///   "assistant" events carry delta text via content[].text.
///   Tool calls come as stream_event → content_block_start/delta/stop.

use super::runner_lifecycle::{run_cli_process, LineAction};
use super::runner_workspace::{
    format_workspace_change_list, record_change, snapshot_workspace, workspace_change_entries,
};
use super::{SkillChunk, ToolLog};
use serde_json::Value;
use tauri::{Emitter, EventTarget};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::runners::{
    build_claude_args, configured_execution_access_mode, is_claude_forbidden_in_read_only,
    is_claude_write_tool, resolve_cwd, summarize_tool_input, ClaudeExecutionMode,
};

// ── Public wrappers ───────────────────────────────────────────────────

pub(crate) async fn claude(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::WorkspaceWrite, true, None,
    ).await
}

pub(crate) async fn claude_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::WorkspaceWrite, false, None,
    ).await
}

/// Like `claude_quiet`, but tags emitted `skill-chunk` events with a subtask ID.
pub(crate) async fn claude_quiet_subtask(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    subtask_id: &str,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::WorkspaceWrite, false, Some(subtask_id),
    ).await
}

pub(crate) async fn claude_read_only(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::ReadOnlyReview, true, None,
    ).await
}

pub(crate) async fn claude_read_only_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::ReadOnlyReview, false, None,
    ).await
}

// ── Core implementation ───────────────────────────────────────────────

async fn claude_with_streaming(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    mode: ClaudeExecutionMode,
    emit_chunks: bool,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    tracing::info!(mode = ?mode, cwd = ?cwd, "spawning claude");
    let mut cmd = Command::new("claude");
    let access_mode = configured_execution_access_mode();
    cmd.args(build_claude_args(prompt, mode, access_mode))
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SSE_PORT")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "100000");
    let resolved_cwd = resolve_cwd(cwd);
    let workspace_before = if matches!(mode, ClaudeExecutionMode::ReadOnlyReview) && cwd.is_some() {
        Some(snapshot_workspace(&resolved_cwd))
    } else {
        None
    };
    cmd.current_dir(&resolved_cwd);

    // Mutable state accumulated during line processing.
    let mut full_text = String::new();
    let mut is_first_chunk = true;
    let mut pending_tool_name: Option<String> = None;
    let mut pending_tool_input = String::new();
    let owned_subtask_id = subtask_id.map(ToString::to_string);

    let process_result = run_cli_process(
        "claude",
        &mut cmd,
        window_label,
        token,
        |line| {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                return LineAction::Continue;
            };
            match v["type"].as_str() {
                Some("stream_event") => {
                    handle_stream_event(
                        &v, mode, &resolved_cwd, window_label, app_handle,
                        &mut pending_tool_name, &mut pending_tool_input,
                    )
                }
                Some("assistant") => {
                    handle_assistant_message(
                        &v, emit_chunks, window_label, app_handle,
                        &mut full_text, &mut is_first_chunk,
                        owned_subtask_id.as_deref(),
                    )
                }
                Some("result") => {
                    handle_result_event(
                        &v, emit_chunks, window_label, app_handle,
                        &mut full_text,
                        owned_subtask_id.as_deref(),
                    )
                }
                _ => LineAction::Continue,
            }
        },
    ).await;

    // If we have output, a non-zero exit is acceptable (claude may exit 1 but still produce text).
    match process_result {
        Ok(()) => {}
        Err(e) if e.starts_with("claude exited with non-zero") && !full_text.is_empty() => {}
        Err(e) => return Err(e),
    }

    // Read-only workspace integrity check.
    if let Some(before) = workspace_before {
        let after = snapshot_workspace(&resolved_cwd);
        let changes = workspace_change_entries(&before, &after);
        if !changes.is_empty() {
            return Err(format!(
                "Claude read-only run modified workspace files: {}",
                format_workspace_change_list(&changes)
            ));
        }
    }

    Ok(full_text)
}

// ── Protocol handlers ────────────────────────────────────────────────

fn handle_stream_event(
    v: &Value,
    mode: ClaudeExecutionMode,
    resolved_cwd: &std::path::PathBuf,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    pending_tool_name: &mut Option<String>,
    pending_tool_input: &mut String,
) -> LineAction {
    let ev = &v["event"];
    match ev["type"].as_str() {
        Some("content_block_start") => {
            let block = &ev["content_block"];
            if block["type"] == "tool_use" {
                *pending_tool_name = block["name"].as_str().map(|s| s.to_string());
                *pending_tool_input = String::new();
            }
        }
        Some("content_block_delta") => {
            let delta = &ev["delta"];
            if delta["type"] == "input_json_delta" {
                if let Some(frag) = delta["partial_json"].as_str() {
                    pending_tool_input.push_str(frag);
                }
            }
        }
        Some("content_block_stop") => {
            if let Some(tool) = pending_tool_name.take() {
                if matches!(mode, ClaudeExecutionMode::ReadOnlyReview)
                    && is_claude_forbidden_in_read_only(&tool)
                {
                    let attempted_target = summarize_tool_input(&tool, pending_tool_input);
                    return LineAction::Error(format!(
                        "Claude read-only run attempted forbidden tool `{tool}` on `{attempted_target}`"
                    ));
                }
                if is_claude_write_tool(&tool) {
                    record_change(&tool, pending_tool_input, resolved_cwd);
                }
                let input = summarize_tool_input(&tool, pending_tool_input);
                let ts = chrono::Utc::now().timestamp_millis() as u64;
                let _ = app_handle.emit_to(
                    EventTarget::webview_window(window_label),
                    "tool-log",
                    ToolLog { agent: "claude".to_string(), tool, input, timestamp: ts },
                );
                *pending_tool_input = String::new();
            }
        }
        _ => {}
    }
    LineAction::Continue
}

fn handle_assistant_message(
    v: &Value,
    emit_chunks: bool,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    full_text: &mut String,
    is_first_chunk: &mut bool,
    subtask_id: Option<&str>,
) -> LineAction {
    if let Some(arr) = v["message"]["content"].as_array() {
        for item in arr {
            if item["type"] == "text" {
                if let Some(text) = item["text"].as_str() {
                    if !text.is_empty() {
                        full_text.push_str(text);
                        let reset = *is_first_chunk;
                        *is_first_chunk = false;
                        if emit_chunks {
                            let _ = app_handle.emit_to(
                                EventTarget::webview_window(window_label),
                                "skill-chunk",
                                SkillChunk {
                                    agent: "claude".to_string(),
                                    text: text.to_string(),
                                    reset,
                                    subtask_id: subtask_id.map(ToString::to_string),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    LineAction::Continue
}

fn handle_result_event(
    v: &Value,
    emit_chunks: bool,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    full_text: &mut String,
    subtask_id: Option<&str>,
) -> LineAction {
    if v["is_error"].as_bool() == Some(true) {
        let msg = v["result"].as_str().unwrap_or("unknown error").to_string();
        return LineAction::Error(format!("Claude error: {msg}"));
    }
    if full_text.is_empty() {
        if let Some(result) = v["result"].as_str() {
            *full_text = result.to_string();
            if emit_chunks {
                let _ = app_handle.emit_to(
                    EventTarget::webview_window(window_label),
                    "skill-chunk",
                    SkillChunk {
                        agent: "claude".to_string(),
                        text: full_text.clone(),
                        reset: true,
                        subtask_id: subtask_id.map(ToString::to_string),
                    },
                );
            }
        }
    }
    LineAction::Continue
}
