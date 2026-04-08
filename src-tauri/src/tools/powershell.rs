/// PowerShellTool — executes PowerShell commands (Windows powershell / cross-platform pwsh).
///
/// Similar to BashTool but targets PowerShell syntax and cmdlets.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

pub struct PowerShellTool;

/// Maximum timeout in milliseconds (10 minutes).
const MAX_TIMEOUT_MS: u64 = 600_000;
/// Default timeout in milliseconds (2 minutes).
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Read-only PowerShell command prefixes. Commands starting with these
/// (case-insensitive) are considered safe for concurrent execution.
const READ_ONLY_PREFIXES: &[&str] = &[
    // PowerShell Get-* cmdlets
    "get-",
    "select-",
    "where-",
    "measure-",
    "test-",
    "resolve-",
    "format-",
    "convertto-",
    "convertfrom-",
    "compare-",
    "group-",
    "sort-",
    "out-string",
    "out-host",
    // Common aliases
    "ls ",
    "ls\n",
    "ls",
    "dir ",
    "dir\n",
    "dir",
    "cat ",
    "type ",
    "echo ",
    "write-output",
    "write-host",
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
    "git reflog",
    // Info
    "pwd",
    "$pwd",
    "$env:",
    "hostname",
    "whoami",
    "[environment]::",
    "[system.environment]::",
];

fn is_read_only_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();
    for prefix in READ_ONLY_PREFIXES {
        if lower.starts_with(prefix) || lower == prefix.trim() {
            return true;
        }
    }
    false
}

const POWERSHELL_DESCRIPTION: &str = r#"Execute a PowerShell command (Windows).

Executes a given PowerShell command with optional timeout. Working directory persists between commands; shell state (variables, functions) does not.

IMPORTANT: This tool is for terminal operations via PowerShell: git, npm, docker, and PS cmdlets. DO NOT use it for file operations (reading, writing, editing, searching, finding files) - use the specialized tools for this instead.

PowerShell Syntax Notes:
  - Variables use $ prefix: $myVar = "value"
  - Escape character is backtick (`), not backslash
  - Use Verb-Noun cmdlet naming: Get-ChildItem, Set-Location, New-Item, Remove-Item
  - Common aliases: ls (Get-ChildItem), cd (Set-Location), cat (Get-Content), rm (Remove-Item)
  - Pipe operator | works similarly to bash but passes objects, not text
  - Use Select-Object, Where-Object, ForEach-Object for filtering and transformation
  - String interpolation: "Hello $name" or "Hello $($obj.Property)"

Interactive and blocking commands (will hang — this tool runs with -NonInteractive):
  - NEVER use Read-Host, Get-Credential, Out-GridView, $Host.UI.PromptForChoice, or pause
  - Destructive cmdlets (Remove-Item, Stop-Process, Clear-Content, etc.) may prompt for confirmation. Add -Confirm:$false when you intend the action to proceed.

Usage notes:
  - The command argument is required.
  - You can specify an optional timeout in milliseconds (max 600000ms / 10 minutes). Default: 120000ms (2 minutes).
  - Avoid using PowerShell to run commands that have dedicated tools (Glob, Grep, Read, Edit, Write)."#;

#[async_trait]
impl Tool for PowerShellTool {
    fn name(&self) -> &'static str {
        "PowerShell"
    }

    fn description(&self) -> &'static str {
        POWERSHELL_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The PowerShell command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000, default 120000)"
                }
            }
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        let command = input["command"].as_str().unwrap_or("");
        is_read_only_command(command)
    }

    fn is_destructive(&self, input: &Value) -> bool {
        let command = input["command"].as_str().unwrap_or("").trim().to_lowercase();
        let destructive_patterns = [
            "remove-item",
            "rm ",
            "rm\t",
            "del ",
            "rmdir ",
            "clear-content",
            "stop-process",
            "git push --force",
            "git push -f",
            "git reset --hard",
            "git clean -f",
            "git branch -d",
        ];
        for pattern in &destructive_patterns {
            if command.starts_with(pattern) || command.contains(&format!(" {pattern}")) {
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

        let workspace = ctx.workspace.to_path_buf();
        let timeout_duration = Duration::from_millis(timeout_ms);

        // Use pwsh on Linux/macOS, powershell on Windows
        let shell = if cfg!(target_os = "windows") {
            "powershell"
        } else {
            "pwsh"
        };

        let child_future = Command::new(shell)
            .arg("-NonInteractive")
            .arg("-Command")
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
            Ok(Err(e)) => ToolResult::err(format!("Failed to execute PowerShell command: {e}")),
            Err(_) => ToolResult::err(format!(
                "Command timed out after {timeout_ms}ms. Consider increasing the timeout."
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_detection() {
        assert!(is_read_only_command("Get-ChildItem"));
        assert!(is_read_only_command("get-childitem"));
        assert!(is_read_only_command("Get-Process | Select-Object Name"));
        assert!(is_read_only_command("Select-Object -Property Name"));
        assert!(is_read_only_command("Where-Object { $_.Name -eq 'foo' }"));
        assert!(is_read_only_command("git log --oneline"));
        assert!(is_read_only_command("git status"));
        assert!(is_read_only_command("ls"));
        assert!(is_read_only_command("dir"));
        assert!(is_read_only_command("pwd"));

        assert!(!is_read_only_command("Remove-Item foo.txt"));
        assert!(!is_read_only_command("Set-Content -Path foo.txt -Value bar"));
        assert!(!is_read_only_command("New-Item -ItemType File -Path foo.txt"));
        assert!(!is_read_only_command("npm install"));
    }

    #[test]
    fn test_destructive_detection() {
        let tool = PowerShellTool;
        assert!(tool.is_destructive(&json!({"command": "Remove-Item foo.txt"})));
        assert!(tool.is_destructive(&json!({"command": "git push --force"})));
        assert!(tool.is_destructive(&json!({"command": "git reset --hard HEAD~1"})));
        assert!(!tool.is_destructive(&json!({"command": "Get-ChildItem"})));
        assert!(!tool.is_destructive(&json!({"command": "git status"})));
    }

    #[test]
    fn test_name() {
        let tool = PowerShellTool;
        assert_eq!(tool.name(), "PowerShell");
        assert!(tool.anthropic_builtin_type().is_none());
    }
}
