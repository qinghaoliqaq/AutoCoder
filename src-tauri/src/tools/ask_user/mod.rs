pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// AskUserQuestion tool — asks the user a question to gather information,
/// clarify ambiguity, understand preferences, or offer choices.
///
/// Stub: in the real system this would emit a Tauri event and block until
/// the user responds via the frontend.
pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &'static str {
        "AskUserQuestion"
    }

    fn description(&self) -> &'static str {
        "Asks the user multiple choice questions to gather information, clarify \
         ambiguity, understand preferences, make decisions or offer them choices. \
         Use this tool when you need to ask the user questions during execution. \
         This allows you to: 1) Gather user preferences or requirements, \
         2) Clarify ambiguous instructions, 3) Get decisions on implementation \
         choices as you work, 4) Offer choices to the user about what direction \
         to take."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["question"],
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user. Should be clear and specific."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let question = match input["question"].as_str() {
            Some(q) if !q.is_empty() => q,
            _ => return ToolResult::err("Missing required parameter: question"),
        };

        // Stub: in the real system, this would:
        // 1. Emit a Tauri event with the question to the frontend
        // 2. Block/await until the user responds
        // 3. Return the user's answer
        ToolResult::ok(format!("Question for user: {question}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn ask_user_returns_question() {
        let tool = AskUserQuestionTool;
        assert_eq!(tool.name(), "AskUserQuestion");
        assert!(tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"question": "Which framework?"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Which framework?"));
    }

    #[tokio::test]
    async fn ask_user_missing_question() {
        let tool = AskUserQuestionTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
    }
}
