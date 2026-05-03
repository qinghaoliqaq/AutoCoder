//! User-configurable hooks — shell commands fired at agent lifecycle events.
//!
//! Inspired by Claude Code's hooks system. Hooks let users wire side
//! effects (linters, notifications, audit logging, policy enforcement)
//! into the agent loop without modifying the agent itself.
//!
//! ## Events
//!
//! * `PreToolUse` — fires before each tool dispatch. Non-zero exit code
//!   blocks the dispatch; the hook's stderr (or stdout if stderr is
//!   empty) is surfaced to the model as the "tool call" result with
//!   `is_error=true`.
//! * `PostToolUse` — fires after a tool returns. The hook's stdout (when
//!   present) is appended to the tool result the model sees, so a
//!   formatter or static-checker can inject structured feedback. Non-zero
//!   exit codes log a warning but do not undo the tool action.
//! * `Stop` — fires once the top-level agent run finishes. Use for
//!   notifications, post-run checks, etc.
//!
//! ## Configuration
//!
//! Hooks are declared under `[hooks]` in `config.toml`:
//!
//! ```toml
//! [[hooks.pre_tool_use]]
//! matcher = "Bash"               # tool name, or "*" for all
//! command = "echo 'pre' >&2"
//!
//! [[hooks.post_tool_use]]
//! matcher = "Edit"
//! command = "/path/to/lint.sh"
//! timeout_secs = 10              # default 30, hard cap 300
//!
//! [[hooks.stop]]
//! matcher = "*"                  # required field; ignored for stop
//! command = "notify-send 'Agent done'"
//! ```
//!
//! ## Payload
//!
//! Each hook command receives a JSON payload on stdin and a few
//! environment variables for shells that don't want to parse JSON:
//!
//! ```text
//! AUTOCODER_HOOK_EVENT     PreToolUse | PostToolUse | Stop
//! AUTOCODER_TOOL_NAME      tool name (PreToolUse / PostToolUse only)
//! AUTOCODER_AGENT_ID       agent id (e.g. "main", or a subtask id)
//! AUTOCODER_WORKSPACE      absolute workspace path
//! ```
//!
//! ## Cross-platform
//!
//! Commands run via `sh -c` on Unix and `cmd /C` on Windows.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 300;

// ── Configuration types ──────────────────────────────────────────────────────

/// One hook entry as written by the user in `config.toml`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HookConfig {
    /// Either an exact tool name (e.g. "Bash") or "*" to match all tools.
    /// Required for `pre_tool_use` and `post_tool_use`; ignored for `stop`.
    #[serde(default = "default_matcher")]
    pub matcher: String,
    /// Shell command to run.
    pub command: String,
    /// Per-hook timeout. Capped at `MAX_TIMEOUT_SECS`.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

fn default_matcher() -> String {
    "*".to_string()
}

impl HookConfig {
    pub fn effective_timeout(&self) -> Duration {
        let secs = self
            .timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS);
        Duration::from_secs(secs)
    }

    /// Returns true if this hook should fire for the given tool name.
    /// `tool_name` is the exact name the agent saw; matchers are exact
    /// string match or `"*"` wildcard.
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        self.matcher == "*" || self.matcher == tool_name
    }
}

/// All hooks declared by the user, grouped by event.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre_tool_use: Vec<HookConfig>,
    #[serde(default)]
    pub post_tool_use: Vec<HookConfig>,
    #[serde(default)]
    pub stop: Vec<HookConfig>,
}

impl HooksConfig {
    /// Hooks for an event, filtered to those whose matcher applies. For
    /// `Stop` the matcher is ignored (every Stop hook fires).
    fn for_event<'a>(&'a self, event: HookEvent, tool_name: Option<&str>) -> Vec<&'a HookConfig> {
        match event {
            HookEvent::PreToolUse => self
                .pre_tool_use
                .iter()
                .filter(|h| tool_name.is_some_and(|n| h.matches_tool(n)))
                .collect(),
            HookEvent::PostToolUse => self
                .post_tool_use
                .iter()
                .filter(|h| tool_name.is_some_and(|n| h.matches_tool(n)))
                .collect(),
            HookEvent::Stop => self.stop.iter().collect(),
        }
    }
}

// ── Runtime types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
}

impl HookEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::Stop => "Stop",
        }
    }
}

/// Outcome of dispatching every matching hook for one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// All hooks (if any) returned exit-0 with no stdout, or there were no
    /// hooks at all. Proceed with the original action.
    Allow,
    /// At least one hook ran, no blocks, but at least one wrote stdout.
    /// The text should be appended to the tool result the model sees.
    AppendContext(String),
    /// A `PreToolUse` hook returned non-zero. The tool dispatch must
    /// short-circuit; the contained text is the reason surfaced to the
    /// model (typically the hook's stderr).
    Block(String),
}

/// JSON payload written to each hook's stdin.
#[derive(Debug, Clone, Serialize)]
pub struct HookPayload<'a> {
    pub event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<&'a Value>,
    /// Tool result the post-hook sees. Present only for PostToolUse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<&'a Value>,
    pub workspace: &'a str,
    pub agent_id: &'a str,
}

/// Inputs to a single hook dispatch — all the metadata the executor
/// needs to invoke matching hooks for one event.
pub struct HookContext<'a> {
    pub event: HookEvent,
    pub tool_name: Option<&'a str>,
    pub tool_input: Option<&'a Value>,
    pub tool_result: Option<&'a Value>,
    pub workspace: &'a Path,
    pub agent_id: &'a str,
    pub token: &'a CancellationToken,
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

/// Run every matching hook for `ctx.event`, sequentially, short-circuiting
/// on the first non-zero `PreToolUse` exit code.
///
/// Sequential execution is intentional: hooks often write to shared
/// resources (logs, the workspace) and ordering is part of the user's
/// mental model. If parallel execution is needed later, the user can
/// chain commands inside one hook entry.
pub async fn dispatch(config: &HooksConfig, ctx: HookContext<'_>) -> HookOutcome {
    let hooks = config.for_event(ctx.event, ctx.tool_name);
    if hooks.is_empty() {
        return HookOutcome::Allow;
    }

    let workspace_str = ctx.workspace.to_string_lossy();
    let payload = HookPayload {
        event: ctx.event.as_str(),
        tool_name: ctx.tool_name,
        tool_input: ctx.tool_input,
        tool_result: ctx.tool_result,
        workspace: &workspace_str,
        agent_id: ctx.agent_id,
    };
    let payload_json = serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{}".to_string());

    let mut accumulated = String::new();

    for hook in hooks {
        match run_one(hook, &payload, &payload_json, ctx.workspace, ctx.token).await {
            Ok(SingleOutcome::Pass(text)) => {
                if !text.is_empty() {
                    if !accumulated.is_empty() {
                        accumulated.push_str("\n\n");
                    }
                    accumulated.push_str(&text);
                }
            }
            Ok(SingleOutcome::Block(reason)) => {
                if ctx.event == HookEvent::PreToolUse {
                    return HookOutcome::Block(reason);
                }
                // PostToolUse / Stop: log, accumulate as warning, but
                // don't roll back.
                tracing::warn!(
                    event = ctx.event.as_str(),
                    "hook returned non-zero (treating as warning): {reason}"
                );
                if !accumulated.is_empty() {
                    accumulated.push_str("\n\n");
                }
                accumulated.push_str("[hook warning] ");
                accumulated.push_str(&reason);
            }
            Err(e) => {
                // A failed *spawn* (bad command, missing shell) is logged
                // and treated as a warning, not a block — we don't want a
                // typo in config.toml to wedge the agent.
                tracing::warn!(
                    event = ctx.event.as_str(),
                    cmd = %hook.command,
                    "hook execution error: {e}"
                );
            }
        }
    }

    if accumulated.is_empty() {
        HookOutcome::Allow
    } else {
        HookOutcome::AppendContext(accumulated)
    }
}

#[derive(Debug)]
enum SingleOutcome {
    Pass(String), // stdout (may be empty)
    Block(String),
}

async fn run_one(
    hook: &HookConfig,
    _payload: &HookPayload<'_>,
    payload_json: &str,
    workspace: &Path,
    token: &CancellationToken,
) -> Result<SingleOutcome, String> {
    let (shell, flag) = shell_cmd();
    let mut cmd = Command::new(shell);
    cmd.arg(flag).arg(&hook.command);
    cmd.current_dir(workspace);
    cmd.env("AUTOCODER_HOOK_EVENT", _payload.event);
    cmd.env("AUTOCODER_AGENT_ID", _payload.agent_id);
    cmd.env("AUTOCODER_WORKSPACE", _payload.workspace);
    if let Some(name) = _payload.tool_name {
        cmd.env("AUTOCODER_TOOL_NAME", name);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        ErrorKind::NotFound => format!("shell `{shell}` not found"),
        _ => format!("spawn `{}`: {e}", hook.command),
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        // Best-effort: a hook that doesn't read stdin closes it early,
        // which makes our write fail with BrokenPipe — not an error.
        let _ = stdin.write_all(payload_json.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let timeout = hook.effective_timeout();
    let wait_fut = child.wait_with_output();

    let output = tokio::select! {
        biased;
        _ = token.cancelled() => return Err("cancelled".to_string()),
        out = tokio::time::timeout(timeout, wait_fut) => match out {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(format!("wait: {e}")),
            Err(_) => return Err(format!("hook timed out after {}s", timeout.as_secs())),
        },
    };

    let status = output.status.code().unwrap_or(-1);
    let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if status == 0 {
        Ok(SingleOutcome::Pass(stdout_text))
    } else {
        let reason = if !stderr_text.is_empty() {
            stderr_text
        } else if !stdout_text.is_empty() {
            stdout_text
        } else {
            format!("hook exited with status {status}")
        };
        Ok(SingleOutcome::Block(reason))
    }
}

#[cfg(windows)]
fn shell_cmd() -> (&'static str, &'static str) {
    ("cmd", "/C")
}

#[cfg(not(windows))]
fn shell_cmd() -> (&'static str, &'static str) {
    ("sh", "-c")
}

// ── Convenience wrappers ─────────────────────────────────────────────────────

/// PreToolUse dispatcher. Returns `Block(reason)` to short-circuit the
/// tool call; `AppendContext(text)` to inject text into the tool's
/// "before" payload (rare); `Allow` to proceed normally.
pub async fn pre_tool_use(
    config: &HooksConfig,
    workspace: &Path,
    token: &CancellationToken,
    tool_name: &str,
    tool_input: &Value,
    agent_id: &str,
) -> HookOutcome {
    dispatch(
        config,
        HookContext {
            event: HookEvent::PreToolUse,
            tool_name: Some(tool_name),
            tool_input: Some(tool_input),
            tool_result: None,
            workspace,
            agent_id,
            token,
        },
    )
    .await
}

/// PostToolUse dispatcher. `AppendContext(text)` means append to the
/// tool result; `Block` is treated as a warning (already logged inside
/// `dispatch`) and surfaces as text to append; `Allow` is a no-op.
pub async fn post_tool_use(
    config: &HooksConfig,
    workspace: &Path,
    token: &CancellationToken,
    tool_name: &str,
    tool_input: &Value,
    tool_result: &Value,
    agent_id: &str,
) -> HookOutcome {
    dispatch(
        config,
        HookContext {
            event: HookEvent::PostToolUse,
            tool_name: Some(tool_name),
            tool_input: Some(tool_input),
            tool_result: Some(tool_result),
            workspace,
            agent_id,
            token,
        },
    )
    .await
}

/// Stop dispatcher. Output is logged; not surfaced to the model since
/// the loop has already returned.
pub async fn stop(
    config: &HooksConfig,
    workspace: &Path,
    token: &CancellationToken,
    agent_id: &str,
) -> HookOutcome {
    dispatch(
        config,
        HookContext {
            event: HookEvent::Stop,
            tool_name: None,
            tool_input: None,
            tool_result: None,
            workspace,
            agent_id,
            token,
        },
    )
    .await
}

/// Convenience: assert a path is absolute (as used in
/// `HookPayload::workspace`). Some callers pass relative paths from
/// tests; canonicalize when feasible.
#[allow(dead_code)]
pub fn workspace_for_payload(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cancel_token() -> CancellationToken {
        CancellationToken::new()
    }

    #[test]
    fn matcher_exact_and_wildcard() {
        let h = HookConfig {
            matcher: "Bash".to_string(),
            command: "true".to_string(),
            timeout_secs: None,
        };
        assert!(h.matches_tool("Bash"));
        assert!(!h.matches_tool("Edit"));

        let h_star = HookConfig {
            matcher: "*".to_string(),
            command: "true".to_string(),
            timeout_secs: None,
        };
        assert!(h_star.matches_tool("Bash"));
        assert!(h_star.matches_tool("Edit"));
        assert!(h_star.matches_tool("Anything"));
    }

    #[test]
    fn effective_timeout_clamps_to_cap() {
        let h = HookConfig {
            matcher: "*".to_string(),
            command: "true".to_string(),
            timeout_secs: Some(99_999),
        };
        assert_eq!(h.effective_timeout(), Duration::from_secs(MAX_TIMEOUT_SECS));

        let h2 = HookConfig {
            matcher: "*".to_string(),
            command: "true".to_string(),
            timeout_secs: Some(0),
        };
        assert_eq!(h2.effective_timeout(), Duration::from_secs(1));

        let h3 = HookConfig {
            matcher: "*".to_string(),
            command: "true".to_string(),
            timeout_secs: None,
        };
        assert_eq!(
            h3.effective_timeout(),
            Duration::from_secs(DEFAULT_TIMEOUT_SECS)
        );
    }

    #[test]
    fn hooks_config_for_event_filters_by_matcher() {
        let cfg = HooksConfig {
            pre_tool_use: vec![
                HookConfig {
                    matcher: "Bash".to_string(),
                    command: "a".to_string(),
                    timeout_secs: None,
                },
                HookConfig {
                    matcher: "Edit".to_string(),
                    command: "b".to_string(),
                    timeout_secs: None,
                },
                HookConfig {
                    matcher: "*".to_string(),
                    command: "c".to_string(),
                    timeout_secs: None,
                },
            ],
            post_tool_use: vec![],
            stop: vec![HookConfig {
                matcher: "ignored".to_string(),
                command: "d".to_string(),
                timeout_secs: None,
            }],
        };

        let pre_bash: Vec<_> = cfg
            .for_event(HookEvent::PreToolUse, Some("Bash"))
            .iter()
            .map(|h| h.command.clone())
            .collect();
        assert_eq!(pre_bash, vec!["a".to_string(), "c".to_string()]);

        let pre_edit: Vec<_> = cfg
            .for_event(HookEvent::PreToolUse, Some("Edit"))
            .iter()
            .map(|h| h.command.clone())
            .collect();
        assert_eq!(pre_edit, vec!["b".to_string(), "c".to_string()]);

        // Stop ignores tool_name
        let stop: Vec<_> = cfg
            .for_event(HookEvent::Stop, None)
            .iter()
            .map(|h| h.command.clone())
            .collect();
        assert_eq!(stop, vec!["d".to_string()]);
    }

    #[tokio::test]
    async fn empty_config_yields_allow() {
        let cfg = HooksConfig::default();
        let token = cancel_token();
        let outcome = dispatch(
            &cfg,
            HookContext {
                event: HookEvent::PreToolUse,
                tool_name: Some("Bash"),
                tool_input: Some(&json!({})),
                tool_result: None,
                workspace: Path::new("/tmp"),
                agent_id: "test",
                token: &token,
            },
        )
        .await;
        assert_eq!(outcome, HookOutcome::Allow);
    }

    // ── Live shell-execution tests ────────────────────────────────────────
    //
    // These spawn `sh` so they're gated to non-Windows. They're cheap (~50ms
    // each) and isolated to /tmp.

    #[cfg(not(windows))]
    #[tokio::test]
    async fn pre_hook_blocks_on_nonzero_exit() {
        let cfg = HooksConfig {
            pre_tool_use: vec![HookConfig {
                matcher: "Bash".to_string(),
                command: "echo refused >&2; exit 1".to_string(),
                timeout_secs: Some(5),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = pre_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Bash",
            &json!({"command": "rm -rf /"}),
            "main",
        )
        .await;
        match outcome {
            HookOutcome::Block(reason) => assert!(reason.contains("refused")),
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn pre_hook_passes_through_when_command_succeeds() {
        let cfg = HooksConfig {
            pre_tool_use: vec![HookConfig {
                matcher: "*".to_string(),
                command: "true".to_string(),
                timeout_secs: Some(5),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome =
            pre_tool_use(&cfg, Path::new("/tmp"), &token, "Bash", &json!({}), "main").await;
        assert_eq!(outcome, HookOutcome::Allow);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn post_hook_appends_stdout() {
        let cfg = HooksConfig {
            post_tool_use: vec![HookConfig {
                matcher: "Edit".to_string(),
                command: "echo 'lint clean'".to_string(),
                timeout_secs: Some(5),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = post_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Edit",
            &json!({}),
            &json!({"content": "ok"}),
            "main",
        )
        .await;
        match outcome {
            HookOutcome::AppendContext(text) => assert_eq!(text, "lint clean"),
            other => panic!("expected AppendContext, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn post_hook_nonzero_logs_warning_does_not_block() {
        let cfg = HooksConfig {
            post_tool_use: vec![HookConfig {
                matcher: "*".to_string(),
                command: "echo trouble >&2; exit 1".to_string(),
                timeout_secs: Some(5),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = post_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Bash",
            &json!({}),
            &json!({"content": "ok"}),
            "main",
        )
        .await;
        // Treated as warning, surfaces as appended text.
        match outcome {
            HookOutcome::AppendContext(text) => {
                assert!(text.contains("hook warning"), "got: {text}");
                assert!(text.contains("trouble"));
            }
            other => panic!("expected AppendContext, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn hook_timeout_does_not_block_subsequent() {
        let cfg = HooksConfig {
            pre_tool_use: vec![
                HookConfig {
                    matcher: "*".to_string(),
                    command: "sleep 5".to_string(),
                    timeout_secs: Some(1),
                },
                HookConfig {
                    matcher: "*".to_string(),
                    command: "true".to_string(),
                    timeout_secs: Some(2),
                },
            ],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = pre_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Bash",
            &json!({}),
            "main",
        )
        .await;
        // First hook timed out → spawn-error path → logged warning, second
        // hook proceeds and returns clean → final outcome is Allow.
        assert_eq!(outcome, HookOutcome::Allow);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn pre_hook_block_short_circuits_subsequent_hooks() {
        let cfg = HooksConfig {
            pre_tool_use: vec![
                HookConfig {
                    matcher: "*".to_string(),
                    command: "echo first-blocked >&2; exit 2".to_string(),
                    timeout_secs: Some(2),
                },
                HookConfig {
                    matcher: "*".to_string(),
                    // If we reached this, the test would fail — block must
                    // short-circuit *before* the second hook runs.
                    command: "false".to_string(),
                    timeout_secs: Some(2),
                },
            ],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = pre_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Bash",
            &json!({}),
            "main",
        )
        .await;
        match outcome {
            HookOutcome::Block(reason) => assert!(reason.contains("first-blocked")),
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn cancellation_aborts_dispatch() {
        let cfg = HooksConfig {
            pre_tool_use: vec![HookConfig {
                matcher: "*".to_string(),
                command: "sleep 5".to_string(),
                timeout_secs: Some(10),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        // Cancel before invoking; dispatch should bail without executing.
        token.cancel();
        let outcome = pre_tool_use(
            &cfg,
            Path::new("/tmp"),
            &token,
            "Bash",
            &json!({}),
            "main",
        )
        .await;
        // The single hook errored (cancelled) → warning logged → no blocks
        // and no append → Allow. The important assertion is that the call
        // returns *immediately* rather than waiting 5 seconds.
        assert_eq!(outcome, HookOutcome::Allow);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn stop_hook_runs_with_no_tool_name() {
        let cfg = HooksConfig {
            stop: vec![HookConfig {
                matcher: "ignored".to_string(),
                command: "echo done".to_string(),
                timeout_secs: Some(5),
            }],
            ..Default::default()
        };
        let token = cancel_token();
        let outcome = stop(&cfg, Path::new("/tmp"), &token, "main").await;
        match outcome {
            HookOutcome::AppendContext(text) => assert_eq!(text, "done"),
            other => panic!("expected AppendContext, got {other:?}"),
        }
    }

    #[test]
    fn payload_serializes_compactly() {
        let payload = HookPayload {
            event: "PreToolUse",
            tool_name: Some("Bash"),
            tool_input: Some(&json!({"command": "ls"})),
            tool_result: None,
            workspace: "/tmp/proj",
            agent_id: "main",
        };
        let s = serde_json::to_string(&payload).unwrap();
        // tool_result is None and skipped via skip_serializing_if.
        assert!(!s.contains("tool_result"));
        assert!(s.contains(r#""event":"PreToolUse""#));
        assert!(s.contains(r#""tool_name":"Bash""#));
        assert!(s.contains(r#""command":"ls""#));
    }

    #[test]
    fn hooks_config_deserializes_minimal_toml() {
        let toml = r#"
[[pre_tool_use]]
matcher = "Bash"
command = "echo hi"

[[post_tool_use]]
command = "echo bye"

[[stop]]
matcher = "*"
command = "echo stop"
"#;
        let cfg: HooksConfig = toml::from_str(toml).expect("parse");
        assert_eq!(cfg.pre_tool_use.len(), 1);
        assert_eq!(cfg.pre_tool_use[0].matcher, "Bash");
        assert_eq!(cfg.post_tool_use.len(), 1);
        // matcher defaulted to "*" when omitted
        assert_eq!(cfg.post_tool_use[0].matcher, "*");
        assert_eq!(cfg.stop.len(), 1);
    }
}
