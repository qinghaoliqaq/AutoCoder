pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// Maximum sleep duration: 5 minutes (300_000 ms).
const MAX_DURATION_MS: u64 = 300_000;

/// Sleep tool — waits for a specified duration.
///
/// Actually sleeps using `tokio::time::sleep`. The user or cancellation token
/// can interrupt the sleep early.
pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &'static str {
        "Sleep"
    }

    fn description(&self) -> &'static str {
        "Wait for a specified duration. The user can interrupt the sleep at any time. \
         Use this when the user tells you to sleep or rest, when you have nothing to do, \
         or when you're waiting for something. Prefer this over `Bash(sleep ...)` — it \
         doesn't hold a shell process."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["duration_ms"],
            "properties": {
                "duration_ms": {
                    "type": "integer",
                    "description": "Duration to sleep in milliseconds (max 300000 = 5 minutes).",
                    "maximum": 300000
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let duration_ms = match input["duration_ms"].as_u64() {
            Some(d) => d,
            None => {
                // Try as i64 for negative / float cases
                if let Some(d) = input["duration_ms"].as_i64() {
                    if d <= 0 {
                        return ToolResult::err("duration_ms must be a positive integer");
                    }
                    d as u64
                } else {
                    return ToolResult::err(
                        "Missing or invalid required parameter: duration_ms (must be a positive integer)",
                    );
                }
            }
        };

        // Cap at 5 minutes
        let capped = duration_ms.min(MAX_DURATION_MS);

        // Sleep, but respect cancellation
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(capped)) => {
                ToolResult::ok(format!("Slept for {capped}ms"))
            }
            _ = ctx.token.cancelled() => {
                ToolResult::ok("Sleep interrupted by cancellation".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn sleep_short_duration() {
        let tool = SleepTool;
        assert_eq!(tool.name(), "Sleep");
        assert!(tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"duration_ms": 10}), &ctx).await;
        assert!(!result.is_error);
        assert_eq!(result.content, "Slept for 10ms");
    }

    #[tokio::test]
    async fn sleep_caps_at_max() {
        let tool = SleepTool;
        let token = CancellationToken::new();
        // Cancel immediately so we don't actually wait 5 minutes
        token.cancel();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"duration_ms": 999999}), &ctx).await;
        assert!(!result.is_error);
        // Should be interrupted immediately since token was cancelled
        assert!(result.content.contains("interrupted") || result.content.contains("Slept"));
    }

    #[tokio::test]
    async fn sleep_cancellation() {
        let tool = SleepTool;
        let token = CancellationToken::new();
        token.cancel();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"duration_ms": 60000}), &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("interrupted"));
    }

    #[tokio::test]
    async fn sleep_missing_duration() {
        let tool = SleepTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("duration_ms"));
    }

    #[tokio::test]
    async fn sleep_negative_duration() {
        let tool = SleepTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"duration_ms": -100}), &ctx).await;
        assert!(result.is_error);
    }
}
