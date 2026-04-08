pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// SendMessage tool — sends a message to another agent (teammate).
///
/// Stub: actual message routing is handled by the orchestration layer.
/// In the real system, messages are delivered to the target agent's inbox
/// and processed in its next tool round.
pub struct SendMessageTool;

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &'static str {
        "SendMessage"
    }

    fn description(&self) -> &'static str {
        "Send a message to another agent. Your plain text output is NOT visible \
         to other agents — to communicate, you MUST call this tool. Messages from \
         teammates are delivered automatically; you don't check an inbox. Refer to \
         teammates by name, never by UUID."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["to", "message"],
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient: teammate name, or \"*\" for broadcast to all teammates."
                },
                "message": {
                    "type": "string",
                    "description": "The message content to send to the target agent."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let to = match input["to"].as_str() {
            Some(t) if !t.is_empty() => t,
            _ => return ToolResult::err("Missing required parameter: to"),
        };

        let message = match input["message"].as_str() {
            Some(m) if !m.is_empty() => m,
            _ => return ToolResult::err("Missing required parameter: message"),
        };

        // Stub: actual message routing is handled by the orchestration layer.
        // In the real system, this would enqueue the message for delivery to
        // the target agent and return a confirmation.
        let _ = (to, message);
        ToolResult::ok("Message routing is handled by the orchestration layer.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn send_message_returns_stub() {
        let tool = SendMessageTool;
        assert_eq!(tool.name(), "SendMessage");
        assert!(!tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(
                json!({"to": "researcher", "message": "start task 1"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("orchestration layer"));
    }

    #[tokio::test]
    async fn send_message_missing_to() {
        let tool = SendMessageTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"message": "hello"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("to"));
    }

    #[tokio::test]
    async fn send_message_missing_message() {
        let tool = SendMessageTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"to": "researcher"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("message"));
    }
}
