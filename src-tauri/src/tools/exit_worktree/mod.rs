pub mod prompt;

/// ExitWorktreeTool — removes a git worktree and cleans up.
///
/// Runs `git worktree remove <path>` in the workspace to remove a previously
/// created worktree. This is a destructive operation.
use super::{Tool, ToolContext, ToolResult, ToolScope};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ExitWorktreeTool;

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &'static str {
        "ExitWorktree"
    }

    fn description(&self) -> &'static str {
        "Remove a git worktree and clean up"
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path of the worktree to remove"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    fn scope(&self) -> ToolScope {
        // `git worktree remove` operates on the main repo's
        // `.git/worktrees/` registry — see `EnterWorktreeTool::scope` for
        // the full rationale.  A subtask calling this could delete a
        // worktree created by a sibling subtask.
        ToolScope::Session
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return ToolResult::err("Missing required parameter: path"),
        };

        // Resolve relative paths against the workspace
        let worktree_path = {
            let p = std::path::Path::new(path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                ctx.workspace.join(p)
            }
        };

        // Validate that the path is within (or adjacent to) the workspace to
        // prevent removing worktrees belonging to unrelated repositories.
        if let (Ok(canon_ws), Ok(canon_wt)) = (
            ctx.workspace.canonicalize(),
            worktree_path
                .canonicalize()
                .or_else(|_| Ok::<_, std::io::Error>(worktree_path.clone())),
        ) {
            let ws_parent = canon_ws.parent().unwrap_or(&canon_ws);
            if !canon_wt.starts_with(&canon_ws) && !canon_wt.starts_with(ws_parent) {
                return ToolResult::err(format!(
                    "Path '{}' is outside the workspace boundary",
                    path
                ));
            }
        }

        let worktree_str = worktree_path.to_string_lossy().to_string();

        // Run git worktree remove
        let output = tokio::process::Command::new("git")
            .args(["worktree", "remove", &worktree_str])
            .current_dir(ctx.workspace)
            .output()
            .await;

        match output {
            Ok(out) => {
                if out.status.success() {
                    ToolResult::ok(format!(
                        "Successfully removed worktree at {worktree_str}. \
                         Session is back in the original workspace."
                    ))
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    // Suggest --force if there are uncommitted changes
                    let hint = if stderr.contains("dirty") || stderr.contains("modified") {
                        " The worktree has uncommitted changes. Commit or discard them first, \
                         or use `git worktree remove --force` manually."
                    } else {
                        ""
                    };
                    ToolResult::err(format!("git worktree remove failed: {stderr}{hint}"))
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
        let tool = ExitWorktreeTool;
        assert_eq!(tool.name(), "ExitWorktree");
        assert!(!tool.is_read_only(&json!({})));
        assert!(tool.is_destructive(&json!({})));
    }

    #[test]
    fn schema_requires_path() {
        let schema = ExitWorktreeTool.input_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("path")));
    }

    #[tokio::test]
    async fn missing_path_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ExitWorktreeTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("path"));
    }

    #[tokio::test]
    async fn nonexistent_worktree_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ExitWorktreeTool
            .execute(json!({"path": "/tmp/no-such-worktree"}), &ctx)
            .await;
        assert!(result.is_error);
    }
}
