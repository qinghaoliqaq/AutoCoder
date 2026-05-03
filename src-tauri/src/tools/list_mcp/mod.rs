/// ListMcpResourcesTool — lists available resources from MCP servers.
///
/// Optionally filters by server name. Returns resource metadata including URI,
/// name, MIME type, description, and originating server.
/// Currently a stub that will be wired up once the MCP client infrastructure is ready.
pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ListMcpResourcesTool;

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &'static str {
        "ListMcpResources"
    }

    fn description(&self) -> &'static str {
        "List available resources from MCP servers. \
         Each resource includes a URI, name, optional MIME type and description, \
         and the server it belongs to. \
         Optionally filter by server name; if not provided, resources from all \
         connected servers are returned."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server_name": {
                    "type": "string",
                    "description": "Optional server name to filter resources by. If not provided, lists resources from all connected MCP servers."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, _input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        // TODO: Query connected MCP clients for their resource lists.
        // When server_name is provided, filter to that server only.
        // let server_name = input.get("server_name").and_then(|v| v.as_str());
        ToolResult::ok("No MCP servers are currently connected.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn make_ctx(workspace: &Path) -> ToolContext<'_> {
        let token = Box::leak(Box::new(CancellationToken::new()));
        ToolContext::new(workspace, false, token)
    }

    #[test]
    fn tool_metadata() {
        let tool = ListMcpResourcesTool;
        assert_eq!(tool.name(), "ListMcpResources");
        assert!(tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[tokio::test]
    async fn execute_stub_no_servers() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({});
        let result = ListMcpResourcesTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result
            .content
            .contains("No MCP servers are currently connected"));
    }

    #[tokio::test]
    async fn execute_stub_with_server_filter() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server" });
        let result = ListMcpResourcesTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result
            .content
            .contains("No MCP servers are currently connected"));
    }

    #[test]
    fn input_schema_server_name_is_optional() {
        let tool = ListMcpResourcesTool;
        let schema = tool.input_schema();
        // server_name should not be in "required"
        assert!(schema.get("required").is_none());
        // But should be in properties
        assert!(schema["properties"]["server_name"].is_object());
    }
}
