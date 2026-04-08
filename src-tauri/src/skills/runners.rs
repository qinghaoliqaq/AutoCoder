/// Low-level CLI runners for Claude Code and Codex.
///
/// All skill modules call into here — they never spawn processes directly.
///
/// Heavy-lifting is delegated to submodules:
///   runner_process   — PID registry, ChildProcessGuard, terminate / kill
///   runner_workspace — workspace snapshot, change tracking, change.log
///   runner_claude    — Claude stream-json protocol implementation
///   runner_codex     — Codex JSON protocol implementation
use crate::config::{AppConfig, ExecutionAccessMode};
use serde_json::Value;
use std::path::PathBuf;

// Re-export for lib.rs which calls runners::kill_registered_processes.
pub(crate) use super::runner_process::kill_registered_processes;

// Re-export runner functions so callers keep using `runners::claude(...)` etc.
pub(crate) use super::runner_claude::{
    claude, claude_quiet, claude_quiet_subtask, claude_read_only, claude_read_only_quiet,
};
pub(crate) use super::runner_codex::{
    codex, codex_read_only, codex_read_only_quiet, codex_read_only_quiet_subtask,
};

/// Hard wall-clock timeout for interactive claude/codex runner sessions.
/// 30 minutes is generous for any single skill invocation.
pub(super) const RUNNER_TIMEOUT_SECS: u64 = 1800;

/// Resolve the working directory for a CLI runner.
/// If an explicit workspace is provided, use it.
/// Otherwise use /tmp — never the Desktop or home dir, to prevent agents
/// from accidentally writing project files to the wrong location.
pub(super) fn resolve_cwd(cwd: Option<&str>) -> PathBuf {
    if let Some(dir) = cwd {
        return PathBuf::from(dir);
    }
    PathBuf::from("/tmp")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodexExecutionMode {
    WorkspaceWrite,
    ReadOnlyReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClaudeExecutionMode {
    WorkspaceWrite,
    ReadOnlyReview,
}

pub(super) fn configured_execution_access_mode() -> ExecutionAccessMode {
    AppConfig::load().features.execution_access_mode
}

pub(super) fn build_codex_args(
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

pub(super) fn build_claude_args(
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a short, human-readable summary from a tool's raw JSON arguments.

pub(super) fn summarize_tool_input(tool: &str, raw_json: &str) -> String {
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

pub(super) fn is_claude_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "Write" | "Edit" | "Create" | "MultiEdit" | "write_file"
    )
}

pub(super) fn is_claude_shell_tool(tool: &str) -> bool {
    matches!(tool, "Bash" | "bash" | "shell")
}

pub(super) fn is_claude_forbidden_in_read_only(tool: &str) -> bool {
    is_claude_write_tool(tool) || is_claude_shell_tool(tool)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;
    use tokio_util::sync::CancellationToken;

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
        use tokio::io::AsyncBufReadExt;
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
