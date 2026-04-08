/// ReadMcpResourceTool — reads a specific resource from an MCP server.
///
/// Fetches resource content by server name and URI. Handles text and binary
/// content types from the MCP protocol.
/// Currently a stub that will be wired up once the MCP client infrastructure is ready.
pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ReadMcpResourceTool;

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &'static str {
        "ReadMcpResource"
    }

    fn description(&self) -> &'static str {
        "Read a resource from an MCP server. \
         Fetches the content of a specific resource identified by its server name \
         and URI. Resources are provided by MCP servers and can include files, \
         database records, API responses, or any other data the server exposes."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["server_name", "uri"],
            "properties": {
                "server_name": {
                    "type": "string",
                    "description": "The name of the MCP server to read the resource from"
                },
                "uri": {
                    "type": "string",
                    "description": "The URI of the resource to read"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let server_name = match input.get("server_name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: server_name"),
        };

        let uri = match input.get("uri").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: uri"),
        };

        // TODO: Route to the actual MCP client to read the resource.
        ToolResult::ok(format!(
            "MCP resource reading is not yet available. Server: {server_name}, URI: {uri}"
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
        let tool = ReadMcpResourceTool;
        assert_eq!(tool.name(), "ReadMcpResource");
        assert!(tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[tokio::test]
    async fn execute_stub_returns_not_available() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "server_name": "my-server",
            "uri": "file:///path/to/resource.txt"
        });
        let result = ReadMcpResourceTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result
            .content
            .contains("MCP resource reading is not yet available"));
        assert!(result.content.contains("my-server"));
        assert!(result.content.contains("file:///path/to/resource.txt"));
    }

    #[tokio::test]
    async fn missing_server_name() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "uri": "file:///resource" });
        let result = ReadMcpResourceTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("server_name"));
    }

    #[tokio::test]
    async fn missing_uri() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server" });
        let result = ReadMcpResourceTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("uri"));
    }
}
