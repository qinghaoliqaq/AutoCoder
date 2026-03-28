/// Low-level CLI runners for Claude Code and Codex.
///
/// These are the only two functions that actually spawn child processes.
/// All skill modules call into here — they never spawn processes directly.
///
/// Claude stream-json protocol:
///   Each "assistant" event carries delta text via content[].text.
///   Tool calls come as stream_event → content_block_start/delta/stop.
///
/// Codex JSON protocol:
///   "item.started" + type "command_execution" → tool call starting.
///   "item.completed" + type "agent_message"   → text reply.

use super::{SkillChunk, ToolLog};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Emitter, EventTarget};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

/// Hard wall-clock timeout for interactive claude/codex runner sessions.
/// 30 minutes is generous for any single skill invocation.
const RUNNER_TIMEOUT_SECS: u64 = 1800;

/// Append a CREATE or MODIFY entry to <workspace>/change.log.
/// Called whenever Claude uses a file-writing tool (Write, Edit, Create, MultiEdit).
/// Silently ignores errors — change.log is best-effort.
fn record_change(tool: &str, raw_json: &str, cwd: &PathBuf) {
    let file_path = if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        v["file_path"].as_str()
            .or_else(|| v["path"].as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    let Some(file_path) = file_path else { return };
    // Resolve to absolute path
    let abs = if std::path::Path::new(&file_path).is_absolute() {
        PathBuf::from(&file_path)
    } else {
        cwd.join(&file_path)
    };
    let kind = match tool {
        "Write" | "Create" | "write_file" => "CREATE",
        _ => "MODIFY", // Edit, MultiEdit, etc.
    };
    let entry = format!("{kind}: {}\n", abs.to_string_lossy());
    let log_path = cwd.join("change.log");
    use std::io::Write as _;
    let _ = std::fs::OpenOptions::new()
        .create(true).append(true).open(&log_path)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

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

// ── Claude ────────────────────────────────────────────────────────────────────

/// Run `claude -p` in stream-json mode, emitting "skill-chunk" events as tokens arrive.
/// Returns the full accumulated text when the process exits.
/// Cancels and kills the child process if `token` is cancelled.
pub(crate) async fn claude(
    prompt:       &str,
    cwd:          Option<&str>,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let mut cmd = Command::new("claude");
    cmd.args(["-p", prompt, "--output-format", "stream-json", "--include-partial-messages"])
       .env_remove("CLAUDECODE")
       .env_remove("CLAUDE_CODE_SSE_PORT")
       .env_remove("CLAUDE_CODE_ENTRYPOINT")
       .env("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "100000")
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());
    cmd.current_dir(resolve_cwd(cwd));

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start `claude`: {e}"))?;
    let stdout = child.stdout.take()
        .ok_or_else(|| "No stdout from `claude`".to_string())?;
    let stderr = child.stderr.take()
        .ok_or_else(|| "No stderr from `claude`".to_string())?;

    // Drain stderr in the background so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines              = BufReader::new(stdout).lines();
    let mut full_text          = String::new();
    let mut is_first_chunk     = true;
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
                                    if matches!(tool.as_str(), "Write" | "Edit" | "Create" | "MultiEdit" | "write_file") {
                                        record_change(&tool, &pending_tool_input, &resolve_cwd(cwd));
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
                    Some("result") => {
                        if v["is_error"].as_bool() == Some(true) {
                            let msg = v["result"].as_str().unwrap_or("unknown error").to_string();
                            return Err(format!("Claude error: {msg}"));
                        }
                        if full_text.is_empty() {
                            if let Some(result) = v["result"].as_str() {
                                full_text = result.to_string();
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
                    _ => {}
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| format!("Wait error for `claude`: {e}"))?;
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
    prompt:       &str,
    cwd:          Option<&str>,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let mut cmd = Command::new("codex");
    cmd.args(["exec", "--skip-git-repo-check", "--full-auto", "--json", prompt])
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());
    cmd.current_dir(resolve_cwd(cwd));

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start `codex`: {e}"))?;
    let stdout = child.stdout.take()
        .ok_or_else(|| "No stdout from `codex`".to_string())?;
    let stderr = child.stderr.take()
        .ok_or_else(|| "No stderr from `codex`".to_string())?;

    // Drain stderr in the background so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines  = BufReader::new(stdout).lines();
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
                        _ => {}
                    }
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| format!("Wait error for `codex`: {e}"))?;
    if !status.success() && output.is_empty() {
        return Err(format!("Codex exited with non-zero status: {status}"));
    }
    Ok(output)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a short, human-readable summary from a tool's raw JSON arguments.
/// Run `claude -p` silently — collects output but emits no skill-chunk events.
/// Used for internal utility rounds (e.g. naming) that should not appear in chat.
pub(crate) async fn claude_silent(
    prompt: &str,
    cwd:    Option<&str>,
) -> Result<String, String> {
    let mut cmd = Command::new("claude");
    cmd.args(["-p", prompt, "--output-format", "stream-json", "--include-partial-messages"])
       .env_remove("CLAUDECODE")
       .env_remove("CLAUDE_CODE_SSE_PORT")
       .env_remove("CLAUDE_CODE_ENTRYPOINT")
       .env("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "100000")
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());
    cmd.current_dir(resolve_cwd(cwd));

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start `claude`: {e}"))?;
    let stdout = child.stdout.take()
        .ok_or_else(|| "No stdout from `claude`".to_string())?;
    let stderr = child.stderr.take()
        .ok_or_else(|| "No stderr from `claude`".to_string())?;

    // Drain stderr so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines     = BufReader::new(stdout).lines();
    let mut full_text = String::new();

    // 30-minute hard timeout — silent calls can involve large codebases.
    let deadline = tokio::time::sleep(Duration::from_secs(1800));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                let _ = child.kill().await;
                return Err("claude_silent timed out after 1800 s".to_string());
            }
            result = lines.next_line() => {
                let Some(line) = result.map_err(|e| format!("Read error from `claude`: {e}"))? else { break };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                match v["type"].as_str() {
                    Some("assistant") => {
                        if let Some(arr) = v["message"]["content"].as_array() {
                            for item in arr {
                                if item["type"] == "text" {
                                    if let Some(text) = item["text"].as_str() {
                                        full_text.push_str(text);
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
                            if let Some(r) = v["result"].as_str() {
                                full_text = r.to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| format!("Wait error for `claude`: {e}"))?;
    if !status.success() && full_text.is_empty() {
        return Err(format!("Claude exited with non-zero status: {status}"));
    }
    Ok(full_text)
}

pub(crate) fn summarize_tool_input(tool: &str, raw_json: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        // Tool-specific primary key (Claude capitalized names + Codex lowercase)
        let specific_key = match tool {
            "Bash" | "bash" | "shell"       => Some("command"),
            "Read" | "read_file"            => Some("file_path"),
            "Write" | "Edit" | "write_file" => Some("file_path"),
            "Glob" | "glob"                 => Some("pattern"),
            "Grep" | "grep"                 => Some("pattern"),
            _                               => None,
        };
        if let Some(key) = specific_key {
            if let Some(val) = v[key].as_str() {
                return val.chars().take(150).collect();
            }
        }
        // Fallback: common argument keys in priority order
        for key in &["command", "cmd", "file_path", "path", "pattern", "query", "input"] {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cwd_explicit() {
        assert_eq!(resolve_cwd(Some("/workspace/app")), PathBuf::from("/workspace/app"));
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

    // ── record_change ─────────────────────────────────────────────────────────

    #[test]
    fn record_change_writes_create_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let json = r#"{"file_path":"src/main.rs"}"#;
        record_change("Write", json, &cwd);
        let log = std::fs::read_to_string(cwd.join("change.log")).unwrap();
        assert!(log.contains("CREATE:"));
        assert!(log.contains("src/main.rs"));
    }

    #[test]
    fn record_change_writes_modify_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let json = r#"{"file_path":"src/lib.rs"}"#;
        record_change("Edit", json, &cwd);
        let log = std::fs::read_to_string(cwd.join("change.log")).unwrap();
        assert!(log.contains("MODIFY:"));
    }

    #[test]
    fn record_change_ignores_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        record_change("Write", "not-json", &cwd);
        // No change.log written — file should not exist
        assert!(!cwd.join("change.log").exists());
    }

    #[test]
    fn record_change_appends_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        record_change("Write",  r#"{"file_path":"a.rs"}"#, &cwd);
        record_change("Edit",   r#"{"file_path":"b.rs"}"#, &cwd);
        let log = std::fs::read_to_string(cwd.join("change.log")).unwrap();
        assert!(log.contains("a.rs"));
        assert!(log.contains("b.rs"));
        assert_eq!(log.lines().count(), 2);
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
