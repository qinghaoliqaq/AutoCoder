/// RemoteTriggerTool — triggers a remote event or webhook.
///
/// This is a stub tool. Remote trigger functionality is not yet implemented;
/// this tool exists as a placeholder for future webhook/event integration.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct RemoteTriggerTool;

const REMOTE_TRIGGER_DESCRIPTION: &str = "Trigger a remote event or webhook. \
Send an event with an optional JSON payload to a remote endpoint. \
Remote triggers are not yet available in this version.";

#[async_trait]
impl Tool for RemoteTriggerTool {
    fn name(&self) -> &'static str {
        "RemoteTrigger"
    }

    fn description(&self) -> &'static str {
        REMOTE_TRIGGER_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["event"],
            "properties": {
                "event": {
                    "type": "string",
                    "description": "The event name or webhook identifier to trigger"
                },
                "payload": {
                    "type": "object",
                    "description": "Optional JSON payload to send with the event"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let event = match input["event"].as_str() {
            Some(e) if !e.trim().is_empty() => e,
            _ => return ToolResult::err("Missing or empty 'event' parameter"),
        };

        let payload_note = if let Some(payload) = input.get("payload") {
            if !payload.is_null() {
                match serde_json::to_string(payload) {
                    Ok(s) => format!(" Payload: {s}"),
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        ToolResult::ok(format!(
            "Remote triggers are not yet available. Event: {event}{payload_note}"
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
        let tool = RemoteTriggerTool;
        assert_eq!(tool.name(), "RemoteTrigger");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[tokio::test]
    async fn test_execute_with_event() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = RemoteTriggerTool
            .execute(json!({"event": "deploy.complete"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("not yet available"));
        assert!(result.content.contains("deploy.complete"));
    }

    #[tokio::test]
    async fn test_execute_with_payload() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = RemoteTriggerTool
            .execute(
                json!({"event": "build.started", "payload": {"branch": "main", "commit": "abc123"}}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("build.started"));
        assert!(result.content.contains("Payload"));
    }

    #[tokio::test]
    async fn test_missing_event() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = RemoteTriggerTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("event"));
    }
}
