/// Shared CLI runner lifecycle — spawn, drain stderr, stream stdout lines
/// with timeout and cancellation, then wait for exit.
///
/// Both Claude and Codex runners share identical process lifecycle management.
/// This module extracts the common plumbing so each runner only needs to
/// implement the protocol-specific JSON line parsing.
use super::runner_process::{isolate_child_process_group, ChildProcessGuard};
use super::runners::RUNNER_TIMEOUT_SECS;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

/// Outcome of processing a single JSON line from a runner's stdout.
pub(super) enum LineAction {
    /// Continue reading lines.
    Continue,
    /// Stop reading — the runner reported a fatal error.
    Error(String),
}

/// Spawn a CLI child process with standard lifecycle management:
///   - Process group isolation for clean kill
///   - PID registration via ChildProcessGuard
///   - Background stderr drain (prevents pipe buffer deadlock)
///   - Cancellation-aware, timeout-aware stdout line loop
///   - Wait for child exit and check status
///
/// The caller provides a `process_line` callback that handles each stdout line
/// according to the runner's specific JSON protocol. The callback receives
/// the raw line string and returns a `LineAction`.
///
/// Returns the final value built up by the caller via the mutable state
/// captured in the `process_line` closure.
pub(super) async fn run_cli_process<F>(
    binary: &str,
    cmd: &mut Command,
    window_label: &str,
    token: CancellationToken,
    mut process_line: F,
) -> Result<(), String>
where
    F: FnMut(&str) -> LineAction,
{
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    isolate_child_process_group(cmd);

    let mut child = cmd.spawn().map_err(|e| {
        tracing::error!(error = %e, "failed to start {binary}");
        format!("Failed to start `{binary}`: {e}")
    })?;
    let _child_guard = ChildProcessGuard::new(window_label, child.id());

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("No stdout from `{binary}`"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("No stderr from `{binary}`"))?;

    // Drain stderr in the background so the child never blocks on a full pipe buffer.
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let mut lines = BufReader::new(stdout).lines();
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
                return Err(format!("{binary} timed out after {RUNNER_TIMEOUT_SECS} s"));
            }
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(l)) => l,
                    Ok(None)    => break,
                    Err(e)      => return Err(format!("Read error from `{binary}`: {e}")),
                };
                match process_line(&line) {
                    LineAction::Continue => {}
                    LineAction::Error(e) => {
                        let _ = child.kill().await;
                        return Err(e);
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Wait error for `{binary}`: {e}"))?;

    if !status.success() {
        // Caller may have accumulated output even with non-zero exit.
        // We report the status; callers that collected output handle it in their wrapper.
        return Err(format!("{binary} exited with non-zero status: {status}"));
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_cli_process_collects_stdout_lines() {
        let token = CancellationToken::new();
        let mut collected = Vec::new();
        let collected_ref = &mut collected;

        // Use `echo` to emit two lines via sh -c.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo line1; echo line2"]);

        let result = run_cli_process("sh", &mut cmd, "test-window", token, |line| {
            collected_ref.push(line.to_string());
            LineAction::Continue
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(collected, vec!["line1", "line2"]);
    }

    #[tokio::test]
    async fn run_cli_process_cancellation_returns_cancelled() {
        let token = CancellationToken::new();
        token.cancel(); // Pre-cancel.

        let mut cmd = Command::new("sleep");
        cmd.arg("999");

        let result = run_cli_process("sleep", &mut cmd, "test-window", token, |_| {
            LineAction::Continue
        })
        .await;

        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[tokio::test]
    async fn run_cli_process_line_error_stops_early() {
        let token = CancellationToken::new();

        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo ok; echo fatal; echo never"]);

        let mut lines_seen = Vec::new();
        let result = run_cli_process("sh", &mut cmd, "test-window", token, |line| {
            lines_seen.push(line.to_string());
            if line == "fatal" {
                LineAction::Error("stopped on fatal".to_string())
            } else {
                LineAction::Continue
            }
        })
        .await;

        assert_eq!(result.unwrap_err(), "stopped on fatal");
        // Should have seen at most "ok" and "fatal" before stopping.
        assert!(lines_seen.contains(&"ok".to_string()));
        assert!(lines_seen.contains(&"fatal".to_string()));
    }

    #[tokio::test]
    async fn run_cli_process_nonzero_exit_returns_error() {
        let token = CancellationToken::new();

        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 42"]);

        let result = run_cli_process("sh", &mut cmd, "test-window", token, |_| {
            LineAction::Continue
        })
        .await;

        let err = result.unwrap_err();
        assert!(err.contains("non-zero status"), "got: {err}");
    }

    #[tokio::test]
    async fn run_cli_process_missing_binary_returns_error() {
        let token = CancellationToken::new();
        let mut cmd = Command::new("__nonexistent_binary_12345__");

        let result = run_cli_process(
            "__nonexistent_binary_12345__",
            &mut cmd,
            "test-window",
            token,
            |_| LineAction::Continue,
        )
        .await;

        let err = result.unwrap_err();
        assert!(err.starts_with("Failed to start"), "got: {err}");
    }
}
