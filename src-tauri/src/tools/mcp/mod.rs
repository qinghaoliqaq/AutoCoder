/// MCPTool — executes a tool provided by an MCP (Model Context Protocol) server.
///
/// Routes tool invocations to connected MCP servers by server name and tool name.
/// Currently a stub that will be wired up once the MCP client infrastructure is ready.
pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct MCPTool;

#[async_trait]
impl Tool for MCPTool {
    fn name(&self) -> &'static str {
        "MCP"
    }

    fn description(&self) -> &'static str {
        "Execute a tool provided by an MCP (Model Context Protocol) server. \
         MCP servers expose additional tools that extend the assistant's capabilities. \
         Use this tool to invoke a specific tool on a named MCP server, passing any \
         arguments the server tool requires."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["server_name", "tool_name"],
            "properties": {
                "server_name": {
                    "type": "string",
                    "description": "The name of the MCP server to execute the tool on"
                },
                "tool_name": {
                    "type": "string",
                    "description": "The name of the tool to execute on the MCP server"
                },
                "arguments": {
                    "type": "object",
                    "description": "Optional arguments to pass to the MCP server tool",
                    "additionalProperties": true
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        // MCP server tools may perform writes; we cannot know at schema time.
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let server_name = match input.get("server_name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: server_name"),
        };

        let tool_name = match input.get("tool_name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: tool_name"),
        };

        // TODO: Route to the actual MCP client infrastructure once available.
        ToolResult::ok(format!(
            "MCP server tool execution is not yet available. Server: {server_name}, Tool: {tool_name}"
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
        let tool = MCPTool;
        assert_eq!(tool.name(), "MCP");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[tokio::test]
    async fn execute_stub_returns_not_available() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({
            "server_name": "my-server",
            "tool_name": "search",
            "arguments": { "query": "hello" }
        });
        let result = MCPTool.execute(input, &ctx).await;
        assert!(!result.is_error);
        assert!(result
            .content
            .contains("MCP server tool execution is not yet available"));
        assert!(result.content.contains("my-server"));
        assert!(result.content.contains("search"));
    }

    #[tokio::test]
    async fn missing_server_name() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "tool_name": "search" });
        let result = MCPTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("server_name"));
    }

    #[tokio::test]
    async fn missing_tool_name() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let input = json!({ "server_name": "my-server" });
        let result = MCPTool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("tool_name"));
    }
}
