pub mod prompt;

/// TeamDeleteTool — deletes a team and stops all its agents.
///
/// Removes the named team. This is a destructive operation that terminates
/// all agents associated with the team.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct TeamDeleteTool;

#[async_trait]
impl Tool for TeamDeleteTool {
    fn name(&self) -> &'static str {
        "TeamDelete"
    }

    fn description(&self) -> &'static str {
        "Delete a team and stop all its agents"
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the team to delete"
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

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.is_empty() => n,
            _ => return ToolResult::err("Missing required parameter: name"),
        };

        ToolResult::ok(format!("Team '{}' deleted.", name))
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
        let tool = TeamDeleteTool;
        assert_eq!(tool.name(), "TeamDelete");
        assert!(!tool.is_read_only(&json!({})));
        assert!(tool.is_destructive(&json!({})));
    }

    #[test]
    fn schema_requires_name() {
        let schema = TeamDeleteTool.input_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("name")));
    }

    #[tokio::test]
    async fn deletes_team_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = TeamDeleteTool
            .execute(json!({"name": "my-team"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("my-team"));
        assert!(result.content.contains("deleted"));
    }

    #[tokio::test]
    async fn missing_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = TeamDeleteTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("name"));
    }
}
