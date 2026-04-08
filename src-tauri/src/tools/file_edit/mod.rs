/// FileEditTool — performs exact string replacements in files.
///
/// Finds `old_string` in the file and replaces it with `new_string`.
/// By default requires exactly one match; `replace_all` replaces every occurrence.
pub mod prompt;

use super::path_utils::resolve_path;
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &'static str {
        "Edit"
    }

    fn description(&self) -> &'static str {
        "Performs exact string replacements in files.\n\n\
         Usage:\n\
         - You must use the Read tool at least once in the conversation before editing. \
         This tool will error if you attempt an edit without reading the file.\n\
         - When editing text from Read tool output, ensure you preserve the exact indentation \
         (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: \
         line number + tab. Everything after that is the actual file content to match. \
         Never include any part of the line number prefix in the old_string or new_string.\n\
         - ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n\
         - The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string \
         with more surrounding context to make it unique or use `replace_all` to change every instance \
         of `old_string`.\n\
         - Use `replace_all` for replacing and renaming strings across the file. \
         This parameter is useful if you want to rename a variable for instance."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "old_string", "new_string"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "default": false,
                    "description": "Replace all occurrences of old_string (default false)"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        // ── Parse input ──────────────────────────────────────────────────
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };
        let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: old_string"),
        };
        let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: new_string"),
        };
        let replace_all = input
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // ── Validate: old_string != new_string ───────────────────────────
        if old_string == new_string {
            return ToolResult::err(
                "No changes to make: old_string and new_string are exactly the same.",
            );
        }

        // ── Resolve & secure path ────────────────────────────────────────
        let resolved = match resolve_path(file_path, ctx.workspace) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        // ── Read existing file ───────────────────────────────────────────
        let content = match tokio::fs::read_to_string(&resolved).await {
            Ok(c) => c,
            Err(e) => {
                return match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        if old_string.is_empty() {
                            // Creating a new file with empty old_string (insert content)
                            match create_file_with_content(&resolved, new_string).await {
                                Ok(()) => ToolResult::ok(format!(
                                    "File created successfully at: {file_path}"
                                )),
                                Err(e) => ToolResult::err(format!("Failed to create file: {e}")),
                            }
                        } else {
                            ToolResult::err(format!("File does not exist: {file_path}"))
                        }
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        ToolResult::err(format!("Permission denied: {file_path}"))
                    }
                    _ => ToolResult::err(format!("Error reading file: {e}")),
                };
            }
        };

        // ── Handle empty old_string on existing file ─────────────────────
        if old_string.is_empty() {
            if content.trim().is_empty() {
                // Empty file — replace empty content with new content
                return match tokio::fs::write(&resolved, new_string).await {
                    Ok(()) => ToolResult::ok(format!(
                        "The file {file_path} has been updated successfully."
                    )),
                    Err(e) => ToolResult::err(format!("Failed to write file: {e}")),
                };
            }
            return ToolResult::err("Cannot create new file - file already exists.");
        }

        // ── Count occurrences ────────────────────────────────────────────
        let match_count = content.matches(old_string).count();

        if match_count == 0 {
            return ToolResult::err(format!(
                "String to replace not found in file.\nString: {old_string}"
            ));
        }

        if match_count > 1 && !replace_all {
            return ToolResult::err(format!(
                "Found {match_count} matches of the string to replace, but replace_all is false. \
                 To replace all occurrences, set replace_all to true. To replace only one occurrence, \
                 please provide more context to uniquely identify the instance.\nString: {old_string}"
            ));
        }

        // ── Perform replacement ──────────────────────────────────────────
        let updated = if replace_all {
            content.replace(old_string, new_string)
        } else {
            // Replace exactly the first (and only) occurrence
            content.replacen(old_string, new_string, 1)
        };

        // ── Write back ──────────────────────────────────────────────────
        match tokio::fs::write(&resolved, &updated).await {
            Ok(()) => {
                if replace_all {
                    ToolResult::ok(format!(
                        "The file {file_path} has been updated. All occurrences were successfully replaced."
                    ))
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

/// Create a new file, including parent directories, and write initial content.
async fn create_file_with_content(
    path: &std::path::Path,
    content: &str,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, content).await
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
    async fn edit_single_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("test.txt");
        std::fs::write(&file, "Hello, world!").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "world",
            "new_string": "Rust"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Hello, Rust!");
    }

    #[tokio::test]
    async fn edit_multiple_without_replace_all_fails() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("dups.txt");
        std::fs::write(&file, "aaa bbb aaa").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "aaa",
            "new_string": "ccc"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("2 matches"));
    }

    #[tokio::test]
    async fn edit_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("multi.txt");
        std::fs::write(&file, "foo bar foo baz foo").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "foo",
            "new_string": "qux",
            "replace_all": true
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "qux bar qux baz qux"
        );
    }

    #[tokio::test]
    async fn edit_string_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("nope.txt");
        std::fs::write(&file, "Hello").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "Goodbye",
            "new_string": "Hi"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn edit_same_strings_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("same.txt");
        std::fs::write(&file, "content").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "content",
            "new_string": "content"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("exactly the same"));
    }

    #[tokio::test]
    async fn edit_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": ws.join("ghost.txt").to_str().unwrap(),
            "old_string": "x",
            "new_string": "y"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("does not exist"));
    }

    #[tokio::test]
    async fn edit_create_new_file_with_empty_old_string() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("subdir").join("new.txt");
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "old_string": "",
            "new_string": "brand new content"
        });
        let result = FileEditTool.execute(input, &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "brand new content");
    }
}
