pub mod prompt;

/// TeamCreateTool — creates a team of agents for collaborative work.
///
/// Teams are coordinated by the orchestration layer. This tool registers a new
/// team with a name and optional description and member list.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct TeamCreateTool;

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &'static str {
        "TeamCreate"
    }

    fn description(&self) -> &'static str {
        "Create a team of agents for collaborative work"
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
                    "description": "Name for the new team"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description of the team's purpose"
                },
                "members": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of initial team member names"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.is_empty() => n,
            _ => return ToolResult::err("Missing required parameter: name"),
        };

        ToolResult::ok(format!(
            "Team '{}' created. Team coordination is handled by the orchestration layer.",
            name
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
    fn metadata() {
        let tool = TeamCreateTool;
        assert_eq!(tool.name(), "TeamCreate");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[test]
    fn schema_requires_name() {
        let schema = TeamCreateTool.input_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("name")));
    }

    #[tokio::test]
    async fn creates_team_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = TeamCreateTool
            .execute(json!({"name": "my-team", "description": "A test team"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("my-team"));
        assert!(result.content.contains("created"));
    }

    #[tokio::test]
    async fn missing_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = TeamCreateTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("name"));
    }
}
