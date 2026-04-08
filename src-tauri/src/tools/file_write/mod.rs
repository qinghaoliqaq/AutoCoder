/// FileWriteTool — writes (creates or overwrites) a file on the local filesystem.
///
/// Creates parent directories as needed. Full content replacement.
pub mod prompt;

use super::path_utils::resolve_path;
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &'static str {
        "Write"
    }

    fn description(&self) -> &'static str {
        "Writes a file to the local filesystem.\n\n\
         Usage:\n\
         - This tool will overwrite the existing file if there is one at the provided path.\n\
         - If this is an existing file, you MUST use the Read tool first to read the file's contents. \
         This tool will fail if you did not read the file first.\n\
         - Prefer the Edit tool for modifying existing files \u{2014} it only sends the diff. \
         Only use this tool to create new files or for complete rewrites.\n\
         - NEVER create documentation files (*.md) or README files unless explicitly requested by the User.\n\
         - Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "content"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    fn is_destructive(&self, input: &Value) -> bool {
        // Destructive when overwriting an existing file.
        // We check synchronously: if the resolved path exists, it's destructive.
        if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str()) {
            let path = std::path::Path::new(file_path);
            path.exists()
        } else {
            false
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        // ── Parse input ──────────────────────────────────────────────────
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };
        let content = match input.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::err("Missing required parameter: content"),
        };

        // ── Resolve & secure path ────────────────────────────────────────
        let resolved = match resolve_path(file_path, ctx.workspace) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        // ── Determine if creating or updating ────────────────────────────
        let is_new = !resolved.exists();

        // ── Create parent directories if needed ──────────────────────────
        if let Some(parent) = resolved.parent() {
            if !parent.exists() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return ToolResult::err(format!(
                        "Failed to create parent directories: {e}"
                    ));
                }
            }
        }

        // ── Write content ────────────────────────────────────────────────
        match tokio::fs::write(&resolved, content).await {
            Ok(()) => {
                if is_new {
                    ToolResult::ok(format!("File created successfully at: {file_path}"))
                } else {
                    ToolResult::ok(format!(
                        "The file {file_path} has been updated successfully."
                    ))
                }
            }
            Err(e) => ToolResult::err(format!("Failed to write file: {e}")),
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

    #[tokio::test]
    async fn write_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("new.txt");
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "content": "Hello, Rust!"
        });
        let result = FileWriteTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert!(result.content.contains("created"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Hello, Rust!");
    }

    #[tokio::test]
    async fn write_overwrite_existing() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("existing.txt");
        std::fs::write(&file, "old content").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "content": "new content"
        });
        let result = FileWriteTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert!(result.content.contains("updated"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("a").join("b").join("c").join("deep.txt");
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "content": "deeply nested"
        });
        let result = FileWriteTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert!(result.content.contains("created"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "deeply nested");
    }

    #[tokio::test]
    async fn write_missing_params() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);

        let result = FileWriteTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("file_path"));

        let result = FileWriteTool
            .execute(json!({ "file_path": "/tmp/x.txt" }), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("content"));
    }

    #[tokio::test]
    async fn is_destructive_for_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("exists.txt");
        std::fs::write(&file, "data").unwrap();

        let input = json!({
            "file_path": file.to_str().unwrap(),
            "content": "new"
        });
        assert!(FileWriteTool.is_destructive(&input));

        let input_new = json!({
            "file_path": ws.join("nonexistent.txt").to_str().unwrap(),
            "content": "new"
        });
        assert!(!FileWriteTool.is_destructive(&input_new));
    }
}
