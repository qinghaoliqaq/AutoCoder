/// Claude CLI runner — spawns `claude -p` in stream-json mode.
///
/// Parses the Claude stream-json protocol:
///   "assistant" events carry delta text via content[].text.
///   Tool calls come as stream_event → content_block_start/delta/stop.

use super::runner_process::{isolate_child_process_group, ChildProcessGuard};
use super::runner_workspace::{
    format_workspace_change_list, record_change, snapshot_workspace, workspace_change_entries,
};
use super::{SkillChunk, ToolLog};
use serde_json::Value;
use tauri::{Emitter, EventTarget};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::runners::{
    build_claude_args, configured_execution_access_mode, is_claude_forbidden_in_read_only,
    is_claude_write_tool, resolve_cwd, summarize_tool_input, ClaudeExecutionMode,
    RUNNER_TIMEOUT_SECS,
};

// ── Public wrappers ───────────────────────────────────────────────────────

pub(crate) async fn claude(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt, cwd, window_label, app_handle, token,
        ClaudeExecutionMode::WorkspaceWrite, true,
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
        ClaudeExecutionMode::WorkspaceWrite, false,
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
        ClaudeExecutionMode::ReadOnlyReview, true,
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
        ClaudeExecutionMode::ReadOnlyReview, false,
    ).await
}

// ── Core implementation ───────────────────────────────────────────────────

async fn claude_with_streaming(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    mode: ClaudeExecutionMode,
    emit_chunks: bool,
) -> Result<String, String> {
    let mut cmd = Command::new("claude");
    let access_mode = configured_execution_access_mode();
    cmd.args(build_claude_args(prompt, mode, access_mode))
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SSE_PORT")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "100000")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let resolved_cwd = resolve_cwd(cwd);
    let workspace_before = if matches!(mode, ClaudeExecutionMode::ReadOnlyReview) && cwd.is_some() {
        Some(snapshot_workspace(&resolved_cwd))
    } else {
        None
    };
    cmd.current_dir(&resolved_cwd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start `claude`: {e}"))?;
    let _child_guard = ChildProcessGuard::new(window_label, child.id());
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "No stdout from `claude`".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "No stderr from `claude`".to_string())?;

    // Drain stderr in the background so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut full_text = String::new();
    let mut is_first_chunk = true;
    let mut pending_tool_name: Option<String> = None;
    let mut pending_tool_input = String::new();

    let timeout = tokio::time::sleep(Duration::from_secs(RUNNER_TIMEOUT_SECS));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                let _ = child.kill().await;
                return Err("cancelled".to_string());
            }
            _ = &mut timeout => {
                let _ = child.kill().await;
                return Err(format!("claude timed out after {RUNNER_TIMEOUT_SECS} s"));
            }
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(l)) => l,
                    Ok(None)    => break,
                    Err(e)      => return Err(format!("Read error from `claude`: {e}")),
                };
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };

                match v["type"].as_str() {
                    Some("stream_event") => {
                        let ev = &v["event"];
                        match ev["type"].as_str() {
                            Some("content_block_start") => {
                                let block = &ev["content_block"];
                                if block["type"] == "tool_use" {
                                    pending_tool_name  = block["name"].as_str().map(|s| s.to_string());
                                    pending_tool_input = String::new();
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
                                        let attempted_target =
                                            summarize_tool_input(&tool, &pending_tool_input);
                                        let _ = child.kill().await;
                                        return Err(format!(
                                            "Claude read-only run attempted forbidden tool `{tool}` on `{attempted_target}`"
                                        ));
                                    }
                                    if is_claude_write_tool(&tool) {
                                        record_change(&tool, &pending_tool_input, &resolved_cwd);
                                    }
                                    let input = summarize_tool_input(&tool, &pending_tool_input);
                                    let ts    = chrono::Utc::now().timestamp_millis() as u64;
                                    let _ = app_handle.emit_to(
                                        EventTarget::webview_window(window_label),
                                        "tool-log",
                                        ToolLog { agent: "claude".to_string(), tool, input, timestamp: ts },
                                    );
                                    pending_tool_input = String::new();
                                }
                            }
                            _ => {}
                        }
                    }
                    Some("assistant") => {
                        if let Some(arr) = v["message"]["content"].as_array() {
                            for item in arr {
                                if item["type"] == "text" {
                                    if let Some(text) = item["text"].as_str() {
                                        if !text.is_empty() {
                                            full_text.push_str(text);
                                            let reset = is_first_chunk;
                                            is_first_chunk = false;
                                            if emit_chunks {
                                                app_handle.emit_to(
                                                    EventTarget::webview_window(window_label),
                                                    "skill-chunk",
                                                    SkillChunk {
                                                        agent: "claude".to_string(),
                                                        text:  text.to_string(),
                                                        reset,
                                                    },
                                                ).map_err(|e| format!("Emit error: {e}"))?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some("result") => {
                        if v["is_error"].as_bool() == Some(true) {
                            let msg = v["result"].as_str().unwrap_or("unknown error").to_string();
                            return Err(format!("Claude error: {msg}"));
                        }
                        if full_text.is_empty() {
                            if let Some(result) = v["result"].as_str() {
                                full_text = result.to_string();
                                if emit_chunks {
                                    app_handle.emit_to(
                                        EventTarget::webview_window(window_label),
                                        "skill-chunk",
                                        SkillChunk {
                                            agent: "claude".to_string(),
                                            text:  full_text.clone(),
                                            reset: true,
                                        },
                                    ).map_err(|e| format!("Emit error: {e}"))?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Wait error for `claude`: {e}"))?;
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
    if !status.success() && full_text.is_empty() {
        return Err(format!("Claude exited with non-zero status: {status}"));
    }
    Ok(full_text)
}
