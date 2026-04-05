/// Low-level CLI runners for Claude Code and Codex.
///
/// These are the only two functions that actually spawn child processes.
/// All skill modules call into here — they never spawn processes directly.
///
/// Heavy-lifting is delegated to submodules:
///   runner_process   — PID registry, ChildProcessGuard, terminate / kill
///   runner_workspace — workspace snapshot, change tracking, change.log
///
/// Claude stream-json protocol:
///   Each "assistant" event carries delta text via content[].text.
///   Tool calls come as stream_event → content_block_start/delta/stop.
///
/// Codex JSON protocol:
///   "item.started" + type "command_execution" → tool call starting.
///   "item.completed" + type "agent_message"   → text reply.
use super::runner_process::{isolate_child_process_group, ChildProcessGuard};
use super::runner_workspace::{
    format_workspace_change_list, record_change, record_workspace_snapshot_diff,
    snapshot_workspace, workspace_change_entries,
};
use super::{SkillChunk, ToolLog};
use crate::config::{AppConfig, ExecutionAccessMode};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Emitter, EventTarget};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

// Re-export for lib.rs which calls runners::kill_registered_processes.
pub(crate) use super::runner_process::kill_registered_processes;

/// Hard wall-clock timeout for interactive claude/codex runner sessions.
/// 30 minutes is generous for any single skill invocation.
const RUNNER_TIMEOUT_SECS: u64 = 1800;

/// Resolve the working directory for a CLI runner.
/// If an explicit workspace is provided, use it.
/// Otherwise use /tmp — never the Desktop or home dir, to prevent agents
/// from accidentally writing project files to the wrong location.
fn resolve_cwd(cwd: Option<&str>) -> PathBuf {
    if let Some(dir) = cwd {
        return PathBuf::from(dir);
    }
    PathBuf::from("/tmp")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodexExecutionMode {
    WorkspaceWrite,
    ReadOnlyReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaudeExecutionMode {
    WorkspaceWrite,
    ReadOnlyReview,
}

fn configured_execution_access_mode() -> ExecutionAccessMode {
    AppConfig::load().features.execution_access_mode
}

fn build_codex_args(
    prompt: &str,
    mode: CodexExecutionMode,
    access_mode: ExecutionAccessMode,
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "--skip-git-repo-check".to_string()];

    match mode {
        CodexExecutionMode::WorkspaceWrite => match access_mode {
            ExecutionAccessMode::Sandbox => args.push("--full-auto".to_string()),
            ExecutionAccessMode::FullAccess => {
                args.push("--dangerously-bypass-approvals-and-sandbox".to_string())
            }
        },
        CodexExecutionMode::ReadOnlyReview => {
            args.push("--sandbox".to_string());
            args.push("read-only".to_string());
        }
    }

    args.push("--json".to_string());
    args.push(prompt.to_string());
    args
}

fn build_claude_args(
    prompt: &str,
    mode: ClaudeExecutionMode,
    access_mode: ExecutionAccessMode,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--include-partial-messages".to_string(),
    ];

    match mode {
        ClaudeExecutionMode::WorkspaceWrite => match access_mode {
            ExecutionAccessMode::Sandbox => {
                args.push("--permission-mode".to_string());
                args.push("acceptEdits".to_string());
            }
            ExecutionAccessMode::FullAccess => {
                args.push("--permission-mode".to_string());
                args.push("bypassPermissions".to_string());
            }
        },
        ClaudeExecutionMode::ReadOnlyReview => {
            args.push("--permission-mode".to_string());
            args.push("plan".to_string());
        }
    }

    args
}

// ── Claude ────────────────────────────────────────────────────────────────────

/// Run `claude -p` in stream-json mode, emitting "skill-chunk" events as tokens arrive.
/// Returns the full accumulated text when the process exits.
/// Cancels and kills the child process if `token` is cancelled.
pub(crate) async fn claude(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        ClaudeExecutionMode::WorkspaceWrite,
        true,
    )
    .await
}

pub(crate) async fn claude_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        ClaudeExecutionMode::WorkspaceWrite,
        false,
    )
    .await
}

pub(crate) async fn claude_read_only(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        ClaudeExecutionMode::ReadOnlyReview,
        true,
    )
    .await
}

pub(crate) async fn claude_read_only_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    claude_with_streaming(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        ClaudeExecutionMode::ReadOnlyReview,
        false,
    )
    .await
}

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

// ── Codex ─────────────────────────────────────────────────────────────────────

/// Run `codex exec` in non-interactive JSON mode, emitting "skill-chunk" events per reply.
/// Returns the full accumulated agent text when the process exits.
/// Cancels and kills the child process if `token` is cancelled.
pub(crate) async fn codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        CodexExecutionMode::WorkspaceWrite,
        true,
    )
    .await
}

pub(crate) async fn codex_read_only(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        CodexExecutionMode::ReadOnlyReview,
        true,
    )
    .await
}

pub(crate) async fn codex_read_only_quiet(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_codex(
        prompt,
        cwd,
        window_label,
        app_handle,
        token,
        CodexExecutionMode::ReadOnlyReview,
        false,
    )
    .await
}

async fn run_codex(
    prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    mode: CodexExecutionMode,
    emit_chunks: bool,
) -> Result<String, String> {
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
        .map_err(|e| format!("Failed to start `codex`: {e}"))?;
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a short, human-readable summary from a tool's raw JSON arguments.

pub(crate) fn summarize_tool_input(tool: &str, raw_json: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        // Tool-specific primary key (Claude capitalized names + Codex lowercase)
        let specific_key = match tool {
            "Bash" | "bash" | "shell" => Some("command"),
            "Read" | "read_file" => Some("file_path"),
            "Write" | "Edit" | "write_file" => Some("file_path"),
            "Glob" | "glob" => Some("pattern"),
            "Grep" | "grep" => Some("pattern"),
            _ => None,
        };
        if let Some(key) = specific_key {
            if let Some(val) = v[key].as_str() {
                return val.chars().take(150).collect();
            }
        }
        // Fallback: common argument keys in priority order
        for key in &[
            "command",
            "cmd",
            "file_path",
            "path",
            "pattern",
            "query",
            "input",
        ] {
            if let Some(val) = v[key].as_str() {
                return val.chars().take(150).collect();
            }
        }
        // Last resort: first string value in the object
        if let Some(obj) = v.as_object() {
            for val in obj.values() {
                if let Some(s) = val.as_str() {
                    return s.chars().take(150).collect();
                }
            }
        }
    }
    raw_json.chars().take(150).collect()
}

fn is_claude_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "Write" | "Edit" | "Create" | "MultiEdit" | "write_file"
    )
}

fn is_claude_shell_tool(tool: &str) -> bool {
    matches!(tool, "Bash" | "bash" | "shell")
}

fn is_claude_forbidden_in_read_only(tool: &str) -> bool {
    is_claude_write_tool(tool) || is_claude_shell_tool(tool)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cwd_explicit() {
        assert_eq!(
            resolve_cwd(Some("/workspace/app")),
            PathBuf::from("/workspace/app")
        );
    }

    #[test]
    fn resolve_cwd_none_is_tmp() {
        assert_eq!(resolve_cwd(None), PathBuf::from("/tmp"));
    }

    #[test]
    fn summarize_bash_picks_command() {
        let json = r#"{"command":"echo hello","other":"ignored"}"#;
        assert_eq!(summarize_tool_input("Bash", json), "echo hello");
    }

    #[test]
    fn summarize_bash_lowercase_alias() {
        let json = r#"{"command":"ls -la"}"#;
        assert_eq!(summarize_tool_input("bash", json), "ls -la");
    }

    #[test]
    fn summarize_read_picks_file_path() {
        let json = r#"{"file_path":"/src/main.rs"}"#;
        assert_eq!(summarize_tool_input("Read", json), "/src/main.rs");
    }

    #[test]
    fn summarize_glob_picks_pattern() {
        let json = r#"{"pattern":"**/*.rs"}"#;
        assert_eq!(summarize_tool_input("Glob", json), "**/*.rs");
    }

    #[test]
    fn summarize_unknown_tool_fallback_to_command_key() {
        let json = r#"{"command":"cargo test"}"#;
        assert_eq!(summarize_tool_input("Unknown", json), "cargo test");
    }

    #[test]
    fn summarize_truncates_at_150_chars() {
        let long = "x".repeat(200);
        let json = format!(r#"{{"command":"{long}"}}"#);
        let result = summarize_tool_input("Bash", &json);
        assert_eq!(result.len(), 150);
    }

    #[test]
    fn summarize_invalid_json_returns_raw() {
        let raw = "not-json";
        assert_eq!(summarize_tool_input("Bash", raw), raw);
    }

    #[test]
    fn summarize_edit_picks_file_path() {
        let json = r#"{"file_path":"src/lib.rs","content":"..."}"#;
        assert_eq!(summarize_tool_input("Edit", json), "src/lib.rs");
    }

    #[test]
    fn summarize_grep_picks_pattern() {
        let json = r#"{"pattern":"fn main"}"#;
        assert_eq!(summarize_tool_input("Grep", json), "fn main");
    }

    #[test]
    fn is_claude_write_tool_matches_all_mutating_tools() {
        for tool in ["Write", "Edit", "Create", "MultiEdit", "write_file"] {
            assert!(is_claude_write_tool(tool));
        }
        assert!(!is_claude_write_tool("Read"));
    }

    #[test]
    fn is_claude_forbidden_in_read_only_blocks_write_and_shell_tools() {
        for tool in [
            "Write",
            "Edit",
            "Create",
            "MultiEdit",
            "write_file",
            "Bash",
            "bash",
            "shell",
        ] {
            assert!(is_claude_forbidden_in_read_only(tool));
        }
        for tool in ["Read", "Glob", "Grep"] {
            assert!(!is_claude_forbidden_in_read_only(tool));
        }
    }

    #[test]
    fn build_codex_args_uses_workspace_write_sandbox_for_write_mode() {
        let args = build_codex_args(
            "review code",
            CodexExecutionMode::WorkspaceWrite,
            ExecutionAccessMode::Sandbox,
        );
        assert_eq!(
            args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--full-auto",
                "--json",
                "review code",
            ]
        );
    }

    #[test]
    fn build_codex_args_uses_full_access_for_write_mode_when_requested() {
        let args = build_codex_args(
            "review code",
            CodexExecutionMode::WorkspaceWrite,
            ExecutionAccessMode::FullAccess,
        );
        assert_eq!(
            args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--dangerously-bypass-approvals-and-sandbox",
                "--json",
                "review code",
            ]
        );
    }

    #[test]
    fn build_codex_args_uses_read_only_sandbox_for_review_mode() {
        let args = build_codex_args(
            "review code",
            CodexExecutionMode::ReadOnlyReview,
            ExecutionAccessMode::FullAccess,
        );
        assert_eq!(
            args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--json",
                "review code",
            ]
        );
    }

    #[test]
    fn build_claude_args_use_accept_edits_in_sandbox_write_mode() {
        let args = build_claude_args(
            "implement feature",
            ClaudeExecutionMode::WorkspaceWrite,
            ExecutionAccessMode::Sandbox,
        );
        assert_eq!(
            args,
            vec![
                "-p",
                "implement feature",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--permission-mode",
                "acceptEdits",
            ]
        );
    }

    #[test]
    fn build_claude_args_use_bypass_permissions_in_full_access_write_mode() {
        let args = build_claude_args(
            "implement feature",
            ClaudeExecutionMode::WorkspaceWrite,
            ExecutionAccessMode::FullAccess,
        );
        assert_eq!(
            args,
            vec![
                "-p",
                "implement feature",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--permission-mode",
                "bypassPermissions",
            ]
        );
    }

    #[test]
    fn build_claude_args_use_plan_mode_for_read_only_review() {
        let args = build_claude_args(
            "review feature",
            ClaudeExecutionMode::ReadOnlyReview,
            ExecutionAccessMode::FullAccess,
        );
        assert_eq!(
            args,
            vec![
                "-p",
                "review feature",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--permission-mode",
                "plan",
            ]
        );
    }

    // ── CancellationToken integration ─────────────────────────────────────────
    // These tests spawn a real `sleep 999` process and cancel the token to verify
    // the cancel chain (token → kill → Err("cancelled")) works end-to-end without
    // requiring the actual `claude` or `codex` binaries.

    #[tokio::test]
    async fn cancellation_kills_child_process() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Spawn a long-lived child (sleep 999) — mirrors what claude/codex runners do
        let mut child = Command::new("sleep")
            .arg("999")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("sleep must be available");

        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();

        // Cancel the token after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            token_clone.cancel();
        });

        let result: Result<(), String> = loop {
            tokio::select! {
                _ = token.cancelled() => {
                    let _ = child.kill().await;
                    break Err("cancelled".to_string());
                }
                line = lines.next_line() => {
                    match line {
                        Ok(None) => break Ok(()),
                        Ok(Some(_)) => {}
                        Err(e) => break Err(e.to_string()),
                    }
                }
            }
        };

        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[tokio::test]
    async fn pre_cancelled_token_returns_immediately() {
        use tokio::process::Command;

        let token = CancellationToken::new();
        token.cancel(); // already cancelled before the run

        let mut child = Command::new("sleep")
            .arg("999")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("sleep must be available");
        let stdout = child.stdout.take().unwrap();
        let mut lines = tokio::io::BufReader::new(stdout).lines();

        let result: Result<(), String> = loop {
            tokio::select! {
                biased; // poll cancellation first
                _ = token.cancelled() => {
                    let _ = child.kill().await;
                    break Err("cancelled".to_string());
                }
                line = lines.next_line() => {
                    match line {
                        Ok(None) => break Ok(()),
                        Ok(Some(_)) => {}
                        Err(e) => break Err(e.to_string()),
                    }
                }
            }
        };

        assert_eq!(result.unwrap_err(), "cancelled");
    }
}
