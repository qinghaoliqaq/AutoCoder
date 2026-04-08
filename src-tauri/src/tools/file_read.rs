/// FileReadTool — reads files with line numbers (cat -n format).
///
/// Supports offset/limit for partial reads, detects binary files,
/// and returns a description for image files.
use super::path_utils::resolve_path;
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Maximum number of lines returned when no explicit limit is given.
const MAX_DEFAULT_LINES: usize = 2000;

/// Common image file extensions.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &'static str {
        "Read"
    }

    fn description(&self) -> &'static str {
        "Reads a file from the local filesystem. You can access any file directly by using this tool.\n\
         Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid.\n\n\
         Usage:\n\
         - The file_path parameter must be an absolute path, not a relative path\n\
         - By default, it reads up to 2000 lines starting from the beginning of the file\n\
         - When you already know which part of the file you need, only read that part. This can be important for larger files.\n\
         - Results are returned using cat -n format, with line numbers starting at 1\n\
         - This tool allows reading images (PNG, JPG, etc). When reading an image file the contents are presented visually.\n\
         - This tool can only read files, not directories. To read a directory, use an ls command via the Bash tool.\n\
         - If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The number of lines to read. Only provide if the file is too large to read at once."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        // ── Parse input ──────────────────────────────────────────────────
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };

        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        // ── Resolve & secure path ────────────────────────────────────────
        let resolved = match resolve_path(file_path, ctx.workspace) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        // ── Check if it's a directory ────────────────────────────────────
        if resolved.is_dir() {
            return ToolResult::err(format!(
                "{} is a directory, not a file. Use the Bash tool with ls to list directory contents.",
                file_path
            ));
        }

        // ── Check if image ───────────────────────────────────────────────
        if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if IMAGE_EXTENSIONS.iter().any(|&img_ext| ext_lower == img_ext) {
                let size = match tokio::fs::metadata(&resolved).await {
                    Ok(m) => m.len(),
                    Err(e) => return ToolResult::err(format!("Failed to read image file: {e}")),
                };
                return ToolResult::ok(format!(
                    "[Image file: {} ({} bytes). Image content is presented visually to the model.]",
                    file_path,
                    size
                ));
            }
        }

        // ── Read file bytes ──────────────────────────────────────────────
        let raw_bytes = match tokio::fs::read(&resolved).await {
            Ok(b) => b,
            Err(e) => {
                return match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        ToolResult::err(format!("File does not exist: {file_path}"))
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        ToolResult::err(format!("Permission denied: {file_path}"))
                    }
                    _ => ToolResult::err(format!("Error reading file: {e}")),
                };
            }
        };

        // ── Detect binary (non-UTF-8) ────────────────────────────────────
        let content = match String::from_utf8(raw_bytes) {
            Ok(s) => s,
            Err(_) => {
                return ToolResult::ok(format!(
                    "File {} appears to be a binary file and cannot be displayed as text.",
                    file_path
                ));
            }
        };

        // ── Handle empty file ────────────────────────────────────────────
        if content.is_empty() {
            return ToolResult::ok(
                "<system-reminder>Warning: the file exists but the contents are empty.</system-reminder>"
            );
        }

        // ── Split into lines and apply offset/limit ──────────────────────
        let all_lines: Vec<&str> = content.split('\n').collect();
        let total_lines = all_lines.len();

        // offset is 0-indexed line number (matching the schema: line number to start from).
        // If not provided, start from the beginning.
        let start = offset.unwrap_or(0);

        if start >= total_lines {
            return ToolResult::ok(format!(
                "<system-reminder>Warning: the file exists but is shorter than the provided offset ({start}). \
                 The file has {total_lines} lines.</system-reminder>"
            ));
        }

        let max_lines = limit.unwrap_or(MAX_DEFAULT_LINES);
        let end = std::cmp::min(start + max_lines, total_lines);
        let selected = &all_lines[start..end];

        // ── Format with line numbers (cat -n style: "{line_num}\t{content}") ─
        let mut result = String::with_capacity(selected.len() * 80);
        for (i, line) in selected.iter().enumerate() {
            // Line numbers are 1-indexed
            let line_num = start + i + 1;
            if i > 0 {
                result.push('\n');
            }
            result.push_str(&format!("{line_num}\t{line}"));
        }

        // ── Append truncation notice if we didn't read all remaining lines ─
        if end < total_lines {
            result.push_str(&format!(
                "\n\n... [{} lines remaining. Use offset to read more.]",
                total_lines - end
            ));
        }

        ToolResult::ok(result)
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
    async fn read_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "file_path": ws.join("nope.txt").to_str().unwrap() });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("does not exist"));
    }

    #[tokio::test]
    async fn read_text_file_with_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("hello.txt");
        std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "file_path": file.to_str().unwrap() });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("1\talpha"));
        assert!(result.content.contains("2\tbeta"));
        assert!(result.content.contains("3\tgamma"));
    }

    #[tokio::test]
    async fn read_with_offset_and_limit() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("lines.txt");
        let content: String = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        std::fs::write(&file, &content).unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "file_path": file.to_str().unwrap(),
            "offset": 10,
            "limit": 5
        });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        // offset=10 means start at 0-indexed line 10 => line number 11
        assert!(result.content.contains("11\tline 11"));
        assert!(result.content.contains("15\tline 15"));
        // Should NOT contain line 16
        assert!(!result.content.contains("16\tline 16"));
    }

    #[tokio::test]
    async fn read_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("empty.txt");
        std::fs::write(&file, "").unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "file_path": file.to_str().unwrap() });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("empty"));
    }

    #[tokio::test]
    async fn read_binary_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("binary.bin");
        std::fs::write(&file, &[0xFF, 0xFE, 0x00, 0x01, 0x80, 0x81]).unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "file_path": file.to_str().unwrap() });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("binary"));
    }

    #[tokio::test]
    async fn read_image_returns_description() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let file = ws.join("screenshot.png");
        std::fs::write(&file, &[0x89, 0x50, 0x4E, 0x47]).unwrap(); // PNG magic bytes
        let ctx = make_ctx(&ws);
        let input = json!({ "file_path": file.to_str().unwrap() });
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("Image file"));
    }

    #[tokio::test]
    async fn missing_file_path_param() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({});
        let result = FileReadTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("file_path"));
    }
}
