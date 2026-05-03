//! `AskUserQuestion` — pause the agent and ask the user a question.
//!
//! Mirrors the `AskUserQuestion` action variant in Warp's agent action enum:
//! the agent suspends, surfaces a question to the UI, and resumes once the
//! user's reply is delivered. The reply registry lives in Tauri-managed
//! state; the frontend acknowledges via the `submit_user_answer` Tauri
//! command.

pub mod registry;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult, ToolScope};
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 300; // 5 minutes
const MAX_TIMEOUT_SECS: u64 = 60 * 60; // 1 hour, hard cap

/// Pluggable backend for delivering a question to the user and awaiting a
/// reply. Production wires to `registry::TauriUserQuestionAsker`; tests
/// inject fakes that resolve immediately.
#[async_trait]
pub trait UserQuestionAsker: Send + Sync {
    async fn ask(&self, request: UserQuestionRequest<'_>) -> Result<String, String>;
}

pub struct UserQuestionRequest<'a> {
    pub app_handle: &'a tauri::AppHandle,
    pub window_label: &'a str,
    pub agent_id: &'a str,
    /// Free-form question shown to the user.
    pub question: &'a str,
    /// Optional preset choices. UI can render these as buttons; user can
    /// also type a free-form reply.
    pub options: &'a [String],
    pub timeout: Duration,
    pub token: tokio_util::sync::CancellationToken,
}

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &'static str {
        "AskUserQuestion"
    }

    fn description(&self) -> &'static str {
        "Pause and ask the user a clarifying question. Use ONLY when you \
         genuinely need information you can't obtain by reading files or \
         running tools — e.g. ambiguous requirements, design choices, or \
         destructive-action confirmations. The tool blocks until the user \
         replies (or the timeout expires); the reply text is returned as \
         the tool result."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["question"],
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The clarifying question to ask the user. \
                                    Be specific and give enough context that \
                                    the user can answer without scrolling back."
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional preset choices the UI may render \
                                    as buttons. The user can also type a \
                                    free-form reply.",
                    "default": []
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_TIMEOUT_SECS,
                    "description": "How long to wait before giving up. \
                                    Defaults to 300 (5 minutes)."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        // Reading user input has no filesystem / shell side effects.
        true
    }

    fn scope(&self) -> ToolScope {
        // Asking the user is a session-level concern — a parallel subtask
        // running in an isolated workspace fork has no business
        // interrupting the human directly.  The orchestrator should ask
        // questions at the main agent layer.
        ToolScope::Session
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(
            "Use this tool sparingly. Prefer making a reasonable default \
             choice and stating your assumption — only ask the user when \
             ambiguity is high enough that a wrong guess would cost real \
             work. Always explain why you're asking, give 2-4 concrete \
             options when possible, and keep the question to one sentence \
             plus context.",
        )
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let question = match input["question"].as_str() {
            Some(q) if !q.trim().is_empty() => q,
            _ => return ToolResult::err("Missing required parameter: question"),
        };

        let options: Vec<String> = input["options"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let timeout_secs = input["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);
        let timeout = Duration::from_secs(timeout_secs);

        let Some(orch) = ctx.orchestration else {
            return ToolResult::err(
                "AskUserQuestion is unavailable: no orchestration context. \
                 This tool can only run inside a real agent loop, not from \
                 isolated subtasks or unit-test contexts.",
            );
        };

        let request = UserQuestionRequest {
            app_handle: orch.app_handle,
            window_label: orch.window_label,
            agent_id: orch.agent_id,
            question,
            options: &options,
            timeout,
            token: ctx.token.clone(),
        };

        match orch.user_question_asker.ask(request).await {
            Ok(answer) => ToolResult::ok(answer),
            Err(e) => ToolResult::err(format!("AskUserQuestion failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::sub_agent_runner::SubAgentRunner;
    use crate::tools::OrchestrationCtx;
    use std::path::Path;
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    /// Fake asker that records the request and returns a canned answer.
    struct FakeAsker {
        answer: String,
        captured: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl UserQuestionAsker for FakeAsker {
        async fn ask(&self, request: UserQuestionRequest<'_>) -> Result<String, String> {
            self.captured
                .lock()
                .unwrap()
                .push(request.question.to_string());
            Ok(self.answer.clone())
        }
    }

    /// SubAgentRunner stub — never called from AskUserQuestion tests but
    /// needed to populate the OrchestrationCtx.
    struct StubRunner;

    #[async_trait]
    impl SubAgentRunner for StubRunner {
        async fn run(
            &self,
            _: crate::tools::sub_agent_runner::SubAgentRequest<'_>,
        ) -> Result<String, String> {
            unreachable!("StubRunner::run should not be invoked in these tests")
        }
    }

    #[tokio::test]
    async fn missing_question_errors() {
        let tool = AskUserQuestionTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("question"));
    }

    #[tokio::test]
    async fn empty_question_errors() {
        let tool = AskUserQuestionTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"question": "   "}), &ctx).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn errors_when_no_orchestration_ctx() {
        let tool = AskUserQuestionTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"question": "What now?"}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("orchestration"));
    }

    #[test]
    fn schema_declares_required_question() {
        let schema = AskUserQuestionTool.input_schema();
        assert_eq!(schema["required"][0], "question");
    }

    #[test]
    fn is_session_scoped_to_block_in_subtasks() {
        // Subtasks run in isolated workspace forks; only the main agent
        // should ever pause the user.
        assert!(matches!(AskUserQuestionTool.scope(), ToolScope::Session));
    }

    #[test]
    fn is_read_only() {
        assert!(AskUserQuestionTool.is_read_only(&json!({})));
    }

    // We can't easily test the orchestration-attached path here without
    // constructing a Tauri AppHandle (which requires a runtime). The
    // `FakeAsker` machinery exists for downstream integration tests; this
    // stub keeps the type alive so future tests have a reference impl.
    #[allow(dead_code)]
    fn _fake_asker_compiles() -> FakeAsker {
        FakeAsker {
            answer: "yes".to_string(),
            captured: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    fn _stub_runner_compiles() -> StubRunner {
        StubRunner
    }

    #[allow(dead_code)]
    fn _orchestration_ctx_compiles<'a>(
        config: &'a crate::config::AppConfig,
        app_handle: &'a tauri::AppHandle,
        runner: &'a dyn SubAgentRunner,
        asker: &'a dyn UserQuestionAsker,
    ) -> OrchestrationCtx<'a> {
        OrchestrationCtx {
            config,
            app_handle,
            window_label: "main",
            agent_id: "main",
            sub_agent_runner: runner,
            user_question_asker: asker,
        }
    }
}
