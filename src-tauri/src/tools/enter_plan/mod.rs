pub mod prompt;

/// EnterPlanModeTool — transitions the session into plan mode for exploration and design.
///
/// In plan mode the agent explores the codebase, designs an approach, and
/// presents a plan for user approval before writing any code.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct EnterPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &'static str {
        "EnterPlanMode"
    }

    fn description(&self) -> &'static str {
        "Enter plan mode to discuss and plan before making changes"
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, _input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        ToolResult::ok(
            "Entered plan mode. Changes will be planned but not executed until you exit plan mode.",
        )
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
        let tool = EnterPlanModeTool;
        assert_eq!(tool.name(), "EnterPlanMode");
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_destructive(&json!({})));
    }

    #[tokio::test]
    async fn execute_returns_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = EnterPlanModeTool.execute(json!({}), &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("Entered plan mode"));
    }
}
