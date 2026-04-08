/// McpAuthTool — manages authentication for MCP servers.
///
/// Supports login, logout, and status actions for MCP server OAuth flows.
/// Currently a stub that will be wired up once the MCP auth infrastructure is ready.
pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct McpAuthTool;

/// Valid authentication actions.
const VALID_ACTIONS: &[&str] = &["login", "logout", "status"];

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &'static str {
        "McpAuth"
    }

    fn description(&self) -> &'static str {
        "Manage authentication for MCP servers. \
         Use this tool to log in, log out, or check the authentication status \
         of a named MCP server. Servers that require OAuth will need authentication \
         before their tools become available."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["server_name", "action"],
            "properties": {
                "server_name": {
                    "type": "string",
                    "description": "The name of the MCP server to manage authentication for"
                },
                "action": {
                    "type": "string",
                    "enum": ["login", "logout", "status"],
                    "description": "The authentication action to perform: \"login\" to start an OAuth flow, \"logout\" to clear credentials, or \"status\" to check current auth state"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        // Auth actions modify state (login/logout), so not read-only.
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let server_name = match input.get("server_name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: server_name"),
        };

        let action = match input.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing required parameter: action"),
        };

        if !VALID_ACTIONS.contains(&action) {
            return ToolResult::err(format!(
                "Invalid action: \"{action}\". Must be one of: login, logout, status"
            ));
        }

        // TODO: Route to the actual MCP auth infrastructure once available.
        ToolResult::ok(format!(
            "MCP authentication is not yet available. Server: {server_name}, Action: {action}"
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
    fn tool_metadata() {
        let tool = McpAuthTool;
        assert_eq!(tool.name(), "McpAuth");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[tokio::test]
    async fn execute_stub_login() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "github-mcp", "action": "login" });
        let result = McpAuthTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result
            .content
            .contains("MCP authentication is not yet available"));
        assert!(result.content.contains("github-mcp"));
        assert!(result.content.contains("login"));
    }

    #[tokio::test]
    async fn execute_stub_status() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server", "action": "status" });
        let result = McpAuthTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("status"));
    }

    #[tokio::test]
    async fn invalid_action() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server", "action": "refresh" });
        let result = McpAuthTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("Invalid action"));
    }

    #[tokio::test]
    async fn missing_server_name() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "action": "login" });
        let result = McpAuthTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("server_name"));
    }

    #[tokio::test]
    async fn missing_action() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server" });
        let result = McpAuthTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("action"));
    }
}
