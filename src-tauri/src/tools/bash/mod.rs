pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

pub struct BashTool;

/// Maximum timeout in milliseconds (10 minutes).
const MAX_TIMEOUT_MS: u64 = 600_000;
/// Default timeout in milliseconds (2 minutes).
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Read-only command prefixes. If a command (trimmed) starts with one of these,
/// it is considered safe for concurrent execution and won't be blocked in
/// read-only mode.
const READ_ONLY_PREFIXES: &[&str] = &[
    // File viewing
    "cat ",
    "head ",
    "tail ",
    "less ",
    "more ",
    // Directory listing
    "ls ",
    "ls\n",
    "ls",
    "tree ",
    "du ",
    "df ",
    // Search
    "grep ",
    "rg ",
    "ag ",
    "ack ",
    "find ",
    "locate ",
    "which ",
    "whereis ",
    // Git read-only
    "git log",
    "git status",
    "git diff",
    "git show",
    "git branch",
    "git tag",
    "git remote",
    "git rev-parse",
    "git describe",
    "git blame",
    "git shortlog",
    "git stash list",
    "git ls-files",
    "git ls-tree",
    "git ls-remote",
    "git cat-file",
    "git name-rev",
    "git reflog",
    "git count-objects",
    // Info / environment
    "pwd",
    "echo ",
    "printf ",
    "env",
    "printenv",
    "whoami",
    "hostname",
    "uname",
    "date",
    "id ",
    "id\n",
    "id",
    // File info
    "wc ",
    "stat ",
    "file ",
    "md5sum ",
    "sha256sum ",
    "sha1sum ",
    "realpath ",
    "readlink ",
    "basename ",
    "dirname ",
    // Process / system info
    "ps ",
    "top ",
    "htop",
    "uptime",
    "free ",
    "lsof ",
    // Package / tool queries
    "npm list",
    "npm ls",
    "npm info",
    "npm view",
    "npm show",
    "npm outdated",
    "npm audit",
    "cargo metadata",
    "cargo tree",
    "pip list",
    "pip show",
    "pip freeze",
    "python --version",
    "python3 --version",
    "node --version",
    "rustc --version",
    "cargo --version",
    "go version",
    "java -version",
    // Docker read-only
    "docker ps",
    "docker images",
    "docker inspect",
    "docker logs",
    // gh read-only
    "gh pr view",
    "gh pr list",
    "gh pr status",
    "gh pr diff",
    "gh pr checks",
    "gh issue view",
    "gh issue list",
    "gh issue status",
    "gh run view",
    "gh run list",
    // Other safe commands
    "type ",
    "command -v ",
    "test ",
    "[ ",
    "true",
    "false",
    "jq ",
    "sort ",
    "uniq ",
    "cut ",
    "tr ",
    "diff ",
];

fn is_read_only_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return true;
    }
    // If the command contains shell chaining operators, it could execute
    // arbitrary commands after the initial read-only prefix. Reject it.
    if trimmed.contains(';')
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('`')
        || trimmed.contains("$(")
    {
        return false;
    }
    for prefix in READ_ONLY_PREFIXES {
        if trimmed.starts_with(prefix) || trimmed == prefix.trim() {
            return true;
        }
    }
    false
}

const BASH_DESCRIPTION: &str = r#"Executes a given bash command and returns its output.

The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh).

IMPORTANT: Avoid using this tool to run `find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task. Instead, use the appropriate dedicated tool as this will provide a much better experience for the user:

 - File search: Use Glob (NOT find or ls)
 - Content search: Use Grep (NOT grep or rg)
 - Read files: Use Read (NOT cat/head/tail)
 - Edit files: Use Edit (NOT sed/awk)
 - Write files: Use Write (NOT echo >/cat <<EOF)
 - Communication: Output text directly (NOT echo/printf)
While the Bash tool can do similar things, it's better to use the built-in tools as they provide a better user experience and make it easier to review tool calls and give permission.

# Instructions
 - If your command will create new directories or files, first use this tool to run `ls` to verify the parent directory exists and is the correct location.
 - Always quote file paths that contain spaces with double quotes in your command (e.g., cd "path with spaces/file.txt")
 - Try to maintain your current working directory throughout the session by using absolute paths and avoiding usage of `cd`. You may use `cd` if the User explicitly requests it.
 - You may specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). By default, your command will timeout after 120000ms (2 minutes).
 - You can use the `run_in_background` parameter to run the command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later. You do not need to check the output right away - you'll be notified when it finishes. You do not need to use '&' at the end of the command when using this parameter.
 - When issuing multiple commands:
   - If the commands are independent and can run in parallel, make multiple Bash tool calls in a single message.
   - If the commands depend on each other and must run sequentially, use a single Bash call with '&&' to chain them together.
   - Use ';' only when you need to run commands sequentially but don't care if earlier commands fail.
   - DO NOT use newlines to separate commands (newlines are ok in quoted strings).
 - For git commands:
   - Prefer to create a new commit rather than amending an existing commit.
   - Before running destructive operations (e.g., git reset --hard, git push --force, git checkout --), consider whether there is a safer alternative that achieves the same goal.
   - Never skip hooks (--no-verify) or bypass signing (--no-gpg-sign, -c commit.gpgsign=false) unless the user has explicitly asked for it.
 - Avoid unnecessary `sleep` commands:
   - Do not sleep between commands that can run immediately — just run them.
   - If your command is long running and you would like to be notified when it finishes — use `run_in_background`. No sleep needed.
   - Do not retry failing commands in a sleep loop — diagnose the root cause.
   - If waiting for a background task you started with `run_in_background`, you will be notified when it completes — do not poll.
   - If you must sleep, keep the duration short (1-5 seconds) to avoid blocking the user."#;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "Bash"
    }

    fn description(&self) -> &'static str {
        BASH_DESCRIPTION
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000, default 120000)"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run the command in the background"
                }
            }
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        let command = input["command"].as_str().unwrap_or("");
        is_read_only_command(command)
    }

    fn is_destructive(&self, input: &Value) -> bool {
        let command = input["command"].as_str().unwrap_or("").trim();
        // Detect obviously destructive commands
        let destructive_prefixes = [
            "rm ",
            "rm\t",
            "rmdir ",
            "git push --force",
            "git push -f",
            "git reset --hard",
            "git checkout .",
            "git clean -f",
            "git branch -D",
        ];
        for prefix in &destructive_prefixes {
            if command.starts_with(prefix) {
                return true;
            }
        }
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let command = match input["command"].as_str() {
            Some(cmd) if !cmd.trim().is_empty() => cmd,
            _ => return ToolResult::err("Missing or empty 'command' parameter"),
        };

        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        let workspace = ctx.workspace.to_path_buf();

        if run_in_background {
            let cmd_owned = command.to_string();
            let ws = workspace.clone();
            tokio::spawn(async move {
                let result = Command::new("sh")
                    .arg("-c")
                    .arg(&cmd_owned)
                    .current_dir(&ws)
                    .kill_on_drop(true)
                    .output()
                    .await;
                match result {
                    Ok(o) => {
                        let code = o.status.code().unwrap_or(-1);
                        if code != 0 {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            tracing::warn!(
                                command = %cmd_owned,
                                exit_code = code,
                                "background command failed: {}",
                                &stderr[..stderr.len().min(512)]
                            );
                        } else {
                            tracing::info!(command = %cmd_owned, "background command completed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(command = %cmd_owned, "background command error: {e}");
                    }
                }
            });
            return ToolResult::ok(format!(
                "Command is running in the background. Note: output is logged \
                 but no real-time notification will be sent.\n\
                 Command: {command}"
            ));
        }

        // Execute the command with timeout
        let timeout_duration = Duration::from_millis(timeout_ms);

        let child_future = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&workspace)
            .output();

        let result = tokio::select! {
            _ = ctx.token.cancelled() => {
                return ToolResult::err("Command cancelled");
            }
            result = tokio::time::timeout(timeout_duration, child_future) => {
                result
            }
        };

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut result_parts = Vec::new();

                if !stdout.is_empty() {
                    result_parts.push(stdout.to_string());
                }

                if !stderr.is_empty() {
                    if !result_parts.is_empty() {
                        result_parts.push(String::new());
                    }
                    result_parts.push(format!("stderr:\n{stderr}"));
                }

                if exit_code != 0 {
                    result_parts.push(format!("\nExit code: {exit_code}"));
                }

                if result_parts.is_empty() {
                    result_parts.push(format!("(no output, exit code {exit_code})"));
                }

                let content = result_parts.join("\n");

                if exit_code == 0 {
                    ToolResult::ok(content)
                } else {
                    ToolResult::err(content)
                }
            }
            Ok(Err(e)) => ToolResult::err(format!("Failed to execute command: {e}")),
            Err(_) => ToolResult::err(format!(
                "Command timed out after {timeout_ms}ms. \
                 Consider increasing the timeout or using run_in_background."
            )),
        }
    }

    fn anthropic_builtin_type(&self) -> Option<&'static str> {
        Some("bash_20250124")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_detection() {
        assert!(is_read_only_command("cat foo.txt"));
        assert!(is_read_only_command("ls -la"));
        assert!(is_read_only_command("  grep -rn pattern ."));
        assert!(is_read_only_command("git log --oneline"));
        assert!(is_read_only_command("git status"));
        assert!(is_read_only_command("git diff HEAD"));
        assert!(is_read_only_command("pwd"));
        assert!(is_read_only_command("echo hello"));
        assert!(is_read_only_command("which cargo"));
        assert!(is_read_only_command("find . -name '*.rs'"));
        assert!(is_read_only_command("gh pr view 123"));

        // Write commands should NOT be read-only
        assert!(!is_read_only_command("rm -rf /"));
        assert!(!is_read_only_command("git push origin main"));
        assert!(!is_read_only_command("npm install"));
        assert!(!is_read_only_command("cargo build"));
        assert!(!is_read_only_command("mkdir new_dir"));
        assert!(!is_read_only_command("touch file.txt"));
    }

    #[test]
    fn test_destructive_detection() {
        let tool = BashTool;
        assert!(tool.is_destructive(&json!({"command": "rm -rf /tmp/test"})));
        assert!(tool.is_destructive(&json!({"command": "git push --force"})));
        assert!(tool.is_destructive(&json!({"command": "git reset --hard HEAD~1"})));
        assert!(!tool.is_destructive(&json!({"command": "ls -la"})));
        assert!(!tool.is_destructive(&json!({"command": "git push origin main"})));
    }

    #[test]
    fn test_name_and_builtin_type() {
        let tool = BashTool;
        assert_eq!(tool.name(), "Bash");
        assert_eq!(tool.anthropic_builtin_type(), Some("bash_20250124"));
    }
}
