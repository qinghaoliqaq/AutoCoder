/// SyntheticOutputTool — produces synthetic/formatted output for display.
///
/// A pass-through tool that returns the provided content as-is, optionally
/// tagged with a format hint. Used by the orchestration layer to surface
/// structured or formatted content to the user interface.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct SyntheticOutputTool;

const SYNTHETIC_OUTPUT_DESCRIPTION: &str = "Produce synthetic/formatted output for display. \
Returns the provided content as-is, optionally with a format hint (text, json, or markdown). \
Use this tool to surface structured or formatted content to the user interface.";

#[async_trait]
impl Tool for SyntheticOutputTool {
    fn name(&self) -> &'static str {
        "SyntheticOutput"
    }

    fn description(&self) -> &'static str {
        SYNTHETIC_OUTPUT_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to output"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "json", "markdown"],
                    "description": "The output format (default: text)"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let content = match input["content"].as_str() {
            Some(c) => c,
            None => return ToolResult::err("Missing required 'content' parameter"),
        };

        let format = input["format"]
            .as_str()
            .unwrap_or("text");

        match format {
            "text" | "json" | "markdown" => {}
            other => {
                return ToolResult::err(format!(
                    "Unsupported format: '{other}'. Supported: text, json, markdown"
                ));
            }
        }

        // Pass-through: return content as-is
        ToolResult::ok(content.to_string())
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
    fn test_metadata() {
        let tool = SyntheticOutputTool;
        assert_eq!(tool.name(), "SyntheticOutput");
        assert!(tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[tokio::test]
    async fn test_pass_through_text() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool
            .execute(json!({"content": "Hello, world!"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert_eq!(result.content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_pass_through_json_format() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool
            .execute(
                json!({"content": "{\"key\": \"value\"}", "format": "json"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert_eq!(result.content, "{\"key\": \"value\"}");
    }

    #[tokio::test]
    async fn test_pass_through_markdown_format() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool
            .execute(
                json!({"content": "# Heading\n\nSome text", "format": "markdown"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("# Heading"));
    }

    #[tokio::test]
    async fn test_default_format_is_text() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool
            .execute(json!({"content": "plain text"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert_eq!(result.content, "plain text");
    }

    #[tokio::test]
    async fn test_unsupported_format() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool
            .execute(
                json!({"content": "test", "format": "xml"}),
                &ctx,
            )
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("Unsupported format"));
    }

    #[tokio::test]
    async fn test_missing_content() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = SyntheticOutputTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("content"));
    }
}
