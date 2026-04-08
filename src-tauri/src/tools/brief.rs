/// BriefTool — generates a brief summary or context document.
///
/// This is a stub tool. Brief generation is handled by the orchestration layer;
/// this tool exists so the model can signal intent to produce a brief.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct BriefTool;

const BRIEF_DESCRIPTION: &str = "Generate a brief summary or context document. \
Provide a topic and optional context to produce a concise brief. \
The actual generation is handled by the orchestration layer.";

#[async_trait]
impl Tool for BriefTool {
    fn name(&self) -> &'static str {
        "Brief"
    }

    fn description(&self) -> &'static str {
        BRIEF_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["topic"],
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "The topic or subject for the brief"
                },
                "context": {
                    "type": "string",
                    "description": "Optional additional context to inform the brief"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let topic = match input["topic"].as_str() {
            Some(t) if !t.trim().is_empty() => t,
            _ => return ToolResult::err("Missing or empty 'topic' parameter"),
        };

        let context_note = match input["context"].as_str() {
            Some(c) if !c.trim().is_empty() => format!(" Context: {c}"),
            _ => String::new(),
        };

        ToolResult::ok(format!(
            "Brief generation is handled by the orchestration layer. Topic: {topic}{context_note}"
        ))
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
        let tool = BriefTool;
        assert_eq!(tool.name(), "Brief");
        assert!(tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[tokio::test]
    async fn test_execute_with_topic() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = BriefTool
            .execute(json!({"topic": "authentication system"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("authentication system"));
        assert!(result.content.contains("orchestration layer"));
    }

    #[tokio::test]
    async fn test_execute_with_context() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = BriefTool
            .execute(
                json!({"topic": "API design", "context": "REST vs GraphQL comparison"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("API design"));
        assert!(result.content.contains("REST vs GraphQL"));
    }

    #[tokio::test]
    async fn test_missing_topic() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = BriefTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("topic"));
    }
}
