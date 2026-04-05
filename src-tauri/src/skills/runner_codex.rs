/// Codex CLI runner — spawns `codex exec` in non-interactive JSON mode.
///
/// Parses the Codex JSON protocol:
///   "item.started" + type "command_execution" → tool call starting.
///   "item.completed" + type "agent_message"   → text reply.

use super::runner_process::{isolate_child_process_group, ChildProcessGuard};
use super::runner_workspace::{record_workspace_snapshot_diff, snapshot_workspace};
use super::{SkillChunk, ToolLog};
use serde_json::Value;
use tauri::{Emitter, EventTarget};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::runners::{
    build_codex_args, configured_execution_access_mode, resolve_cwd, CodexExecutionMode,
    RUNNER_TIMEOUT_SECS,
};

// ── Public wrappers ───────────────────────────────────────────────────────

pub(crate) async fn codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt, cwd, window_label, app_handle, token,
        CodexExecutionMode::WorkspaceWrite, true,
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
        CodexExecutionMode::ReadOnlyReview, true,
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
        CodexExecutionMode::ReadOnlyReview, false,
    ).await
}

// ── Core implementation ───────────────────────────────────────────────────

async fn run_codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    mode: CodexExecutionMode,
    emit_chunks: bool,
) -> Result<String, String> {
    tracing::info!(mode = ?mode, cwd = ?cwd, "spawning codex");
    let mut cmd = Command::new("codex");
    let access_mode = configured_execution_access_mode();
    cmd.args(build_codex_args(prompt, mode, access_mode))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let resolved_cwd = resolve_cwd(cwd);
    let workspace_before = if matches!(mode, CodexExecutionMode::WorkspaceWrite) && cwd.is_some() {
        Some(snapshot_workspace(&resolved_cwd))
    } else {
        None
    };
    cmd.current_dir(&resolved_cwd);

    let mut child = cmd
        .spawn()
        .map_err(|e| {
            tracing::error!(error = %e, "failed to start codex");
            format!("Failed to start `codex`: {e}")
        })?;
    let _child_guard = ChildProcessGuard::new(window_label, child.id());
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "No stdout from `codex`".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "No stderr from `codex`".to_string())?;

    // Drain stderr in the background so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut output = String::new();

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
                return Err(format!("codex timed out after {RUNNER_TIMEOUT_SECS} s"));
            }
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(l)) => l,
                    Ok(None)    => break,
                    Err(e)      => return Err(format!("Read error from `codex`: {e}")),
                };
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                let ev_type = v["type"].as_str().unwrap_or("");

                if ev_type == "error" {
                    let msg = v["message"].as_str()
                        .or_else(|| v["error"].as_str())
                        .unwrap_or("unknown error")
                        .to_string();
                    return Err(format!("Codex error: {msg}"));
                }

                if let Some(item) = v.get("item") {
                    match (ev_type, item["type"].as_str().unwrap_or("")) {
                        ("item.started", "command_execution") => {
                            let command = item["command"].as_str().unwrap_or("").to_string();
                            if !command.is_empty() {
                                let ts = chrono::Utc::now().timestamp_millis() as u64;
                                let _ = app_handle.emit_to(
                                    EventTarget::webview_window(window_label),
                                    "tool-log",
                                    ToolLog {
                                        agent:     "codex".to_string(),
                                        tool:      "Shell".to_string(),
                                        input:     command.chars().take(150).collect(),
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
                                    app_handle.emit_to(
                                        EventTarget::webview_window(window_label),
                                        "skill-chunk",
                                        SkillChunk {
                                            agent: "codex".to_string(),
                                            text:  chunk,
                                            reset: true,
                                        },
                                    ).map_err(|e| format!("Emit error: {e}"))?;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Wait error for `codex`: {e}"))?;
    if let Some(before) = workspace_before {
        let after = snapshot_workspace(&resolved_cwd);
        record_workspace_snapshot_diff(&resolved_cwd, &before, &after);
    }
    if !status.success() && output.is_empty() {
        return Err(format!("Codex exited with non-zero status: {status}"));
    }
    Ok(output)
}
