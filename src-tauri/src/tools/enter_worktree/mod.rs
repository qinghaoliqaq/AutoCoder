pub mod prompt;

/// EnterWorktreeTool — creates an isolated git worktree for safe experimentation.
///
/// Creates a new git worktree using `git worktree add` and reports the path
/// and branch name. If no branch is specified, generates a timestamped name.
use super::{Tool, ToolContext, ToolResult, ToolScope};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct EnterWorktreeTool;

#[async_trait]
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &'static str {
        "EnterWorktree"
    }

    fn description(&self) -> &'static str {
        "Create an isolated git worktree for safe experimentation"
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "branch": {
                    "type": "string",
                    "description": "Branch name for the worktree. If not provided, a timestamped name is generated."
                },
                "path": {
                    "type": "string",
                    "description": "Path for the worktree directory. Defaults to .worktrees/<branch_name> under the workspace."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    fn scope(&self) -> ToolScope {
        // `git worktree add` walks upward from the current directory to
        // find `.git`.  When called from inside an isolated subtask
        // workspace (`.ai-dev-hub/subtasks/<id>/attempt-N/`, which has no
        // `.git` of its own — it's excluded from the fork), git finds the
        // MAIN project repo's `.git` and registers the new worktree in
        // `main_repo/.git/worktrees/`.  That leaks state outside the
        // subtask's isolated copy and races with sibling subtasks that
        // also call this tool.  Session-scope keeps it confined to the
        // main orchestrator where there's exactly one caller.
        ToolScope::Session
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        // Determine branch name
        let branch = match input.get("branch").and_then(|v| v.as_str()) {
            Some(b) if !b.is_empty() => b.to_string(),
            _ => {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                format!("autocoder-worktree-{ts}")
            }
        };

        // Validate branch name to prevent flag injection (e.g. "--orphan")
        if branch.starts_with('-') || branch.contains("..") {
            return ToolResult::err(format!(
                "Invalid branch name: \"{branch}\". Branch names must not start with '-' or contain '..'."
            ));
        }

        // Determine worktree path
        let worktree_path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => {
                let p = std::path::Path::new(p);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    ctx.workspace.join(p)
                }
            }
            _ => ctx.workspace.join(".worktrees").join(&branch),
        };

        // Validate that the worktree path is within (or adjacent to) the
        // workspace to prevent creating worktrees at arbitrary locations.
        if let Ok(canon_ws) = ctx.workspace.canonicalize() {
            let ws_parent = canon_ws.parent().unwrap_or(&canon_ws);
            let check_path = worktree_path
                .canonicalize()
                .unwrap_or_else(|_| worktree_path.clone());
            if !check_path.starts_with(&canon_ws) && !check_path.starts_with(ws_parent) {
                return ToolResult::err(format!(
                    "Path '{}' is outside the workspace boundary",
                    worktree_path.display()
                ));
            }
        }

        let worktree_str = worktree_path.to_string_lossy().to_string();

        // Create parent directory if needed
        if let Some(parent) = worktree_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::err(format!("Failed to create worktree parent directory: {e}"));
            }
        }

        // Run git worktree add
        let output = tokio::process::Command::new("git")
            .args(["worktree", "add", &worktree_str, "-b", &branch])
            .current_dir(ctx.workspace)
            .output()
            .await;

        match output {
            Ok(out) => {
                if out.status.success() {
                    ToolResult::ok(format!(
                        "Created worktree at {worktree_str} on branch {branch}. \
                         The session is now working in the worktree. \
                         Use ExitWorktree to leave when done."
                    ))
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    ToolResult::err(format!("git worktree add failed: {stderr}"))
                }
            }
            Err(e) => ToolResult::err(format!("Failed to run git: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn make_ctx(workspace: &Path) -> ToolContext<'_> {
        let token = Box::leak(Box::new(CancellationToken::new()));
        ToolContext {
            workspace,
            read_only: false,
            token,
        }
    }

    #[test]
    fn metadata() {
        let tool = EnterWorktreeTool;
        assert_eq!(tool.name(), "EnterWorktree");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[test]
    fn schema_has_optional_params() {
        let schema = EnterWorktreeTool.input_schema();
        assert!(schema.get("required").is_none());
        let props = schema.get("properties").unwrap();
        assert!(props.get("branch").is_some());
        assert!(props.get("path").is_some());
    }

    #[tokio::test]
    async fn execute_outside_git_repo_fails() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = EnterWorktreeTool
            .execute(json!({"branch": "test-branch"}), &ctx)
            .await;
        // Should fail because this temp dir is not a git repo
        assert!(result.is_error);
    }
}
