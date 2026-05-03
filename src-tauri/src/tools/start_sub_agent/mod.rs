//! `StartSubAgent` — spawn a fresh agent loop, return its final output.
//!
//! Mirrors the `StartAgent` action variant in Warp's agent action enum:
//! the parent suspends, a new agent runs with its own context window and a
//! caller-provided prompt, and the parent resumes with the sub-agent's
//! reply as the tool result. Compared to free-form delegation via
//! `bash` + scripting, this preserves event streaming, tool gating, and
//! token-usage accounting per sub-agent.
//!
//! The loop dispatch is pluggable through `SubAgentRunner`; production
//! wires it to `tool_runner::run_subtask`, tests inject fakes.
//!
//! ### Why parent-wait-child instead of full mailbox semantics?
//!
//! Warp's full design also has `SendMessageToAgent` for asynchronous
//! mailbox-style coordination between long-lived sub-agents. We're
//! deliberately starting with the synchronous spawn-and-wait subset
//! because:
//!
//! 1. It covers the common case of "do this isolated chunk of work for me"
//!    without introducing a new agent-process lifecycle.
//! 2. It reuses the existing subtask machinery and isolated-workspace
//!    fork in `skills::isolated_workspace` with no orchestrator changes.
//! 3. Mailbox semantics need a router, addressable agents, and the UX to
//!    surface multiple concurrent agents — that's a Phase 3 conversation.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::sub_agent_runner::SubAgentRequest;
use super::{Tool, ToolContext, ToolResult, ToolScope};

const DEFAULT_AGENT_LABEL_LEN: usize = 24;

pub struct StartSubAgentTool;

#[async_trait]
impl Tool for StartSubAgentTool {
    fn name(&self) -> &'static str {
        "StartSubAgent"
    }

    fn description(&self) -> &'static str {
        "Spawn a sub-agent with its own context window to handle a focused, \
         well-scoped task. The sub-agent runs the full tool-use loop and \
         returns its final text output as the tool result. Use this to \
         offload heavy exploration, parallel investigations, or self- \
         contained subtasks so the parent agent's context stays clean. \
         Prefer one focused prompt per call over batching unrelated work."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Self-contained task description for the sub-agent. \
                                    Write it like a colleague brief: state the goal, \
                                    relevant context, expected output format, and \
                                    any non-obvious constraints. The sub-agent does \
                                    not see the parent conversation."
                },
                "name": {
                    "type": "string",
                    "description": "Short label used to tag the sub-agent's events \
                                    in the UI. Auto-generated from the prompt if omitted."
                },
                "system_prompt": {
                    "type": "string",
                    "description": "Optional system prompt prepended to the agent's \
                                    base prompt. Use to install task-specific persona \
                                    or constraints (e.g. \"act as a security reviewer\").",
                    "default": ""
                },
                "read_only": {
                    "type": "boolean",
                    "description": "Run the sub-agent in read-only mode (no Bash, no \
                                    file writes). Use for exploration / review tasks \
                                    where you want strict guarantees the sub-agent \
                                    won't mutate the workspace.",
                    "default": false
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        // The sub-agent itself may write — only `read_only=true` makes
        // this invocation read-safe. Defaulting to false matches the
        // conservative interpretation: if we're not told otherwise,
        // assume mutations are possible.
        input["read_only"].as_bool().unwrap_or(false)
    }

    fn scope(&self) -> ToolScope {
        // Sub-agents must be spawned from the main agent loop, never from
        // inside a parallel subtask's isolated workspace fork.  Nested
        // spawning is a Phase 3+ feature once we have a real router.
        ToolScope::Session
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(
            "Use StartSubAgent for tasks that benefit from a clean context \
             — e.g. \"audit X for security issues\", \"write unit tests for \
             module Y\", \"investigate why test Z is failing.\" The sub- \
             agent does not inherit your message history, so the prompt must \
             stand on its own. The sub-agent's reply is returned as the \
             tool result; treat it as a research report, not a completed \
             work product — verify before you act on its findings.",
        )
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let prompt = match input["prompt"].as_str() {
            Some(p) if !p.trim().is_empty() => p,
            _ => return ToolResult::err("Missing required parameter: prompt"),
        };

        let system_prompt = input["system_prompt"].as_str().unwrap_or("");
        let read_only = input["read_only"].as_bool().unwrap_or(false);
        let label = input["name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| derive_label(prompt));

        let Some(orch) = ctx.orchestration else {
            return ToolResult::err(
                "StartSubAgent is unavailable: no orchestration context. \
                 This tool can only run inside a real agent loop, not from \
                 isolated subtasks or unit-test contexts.",
            );
        };

        // Tag sub-agent events with parent + label so the UI can group them.
        let subtask_id = format!("{}::sub::{label}", orch.agent_id);

        let request = SubAgentRequest {
            config: orch.config,
            app_handle: orch.app_handle,
            workspace: ctx.workspace,
            window_label: orch.window_label,
            system_prompt,
            user_prompt: prompt,
            subtask_id: &subtask_id,
            read_only,
            token: ctx.token.clone(),
        };

        match orch.sub_agent_runner.run(request).await {
            Ok(output) => ToolResult::ok(format!(
                "# Sub-agent `{label}` completed\n\n{output}"
            )),
            Err(e) => ToolResult::err(format!("Sub-agent `{label}` failed: {e}")),
        }
    }
}

/// Auto-derive a label from the first words of the prompt. Sanitizes to a
/// kebab-case slug bounded to `DEFAULT_AGENT_LABEL_LEN` chars so it's
/// usable as part of an event id.
fn derive_label(prompt: &str) -> String {
    let mut out = String::with_capacity(DEFAULT_AGENT_LABEL_LEN);
    let mut prev_was_dash = false;
    for c in prompt.trim().chars() {
        if out.len() >= DEFAULT_AGENT_LABEL_LEN {
            break;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_was_dash = false;
        } else if !prev_was_dash && !out.is_empty() {
            out.push('-');
            prev_was_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-').to_string();
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ask_user_question::UserQuestionAsker;
    use crate::tools::sub_agent_runner::{SubAgentRequest, SubAgentRunner};
    use crate::tools::OrchestrationCtx;
    use std::path::Path;
    use std::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    /// Records the request it receives and returns a canned response.
    struct FakeRunner {
        response: Result<String, String>,
        captured: Mutex<Vec<CapturedRequest>>,
    }

    /// Snapshot of a `SubAgentRequest` for assertions in future end-to-end
    /// tests. Fields are populated by the fake but unused today — the
    /// orchestrated path requires a Tauri AppHandle that we can't easily
    /// build at unit-test scope.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct CapturedRequest {
        user_prompt: String,
        system_prompt: String,
        subtask_id: String,
        read_only: bool,
    }

    #[async_trait]
    impl SubAgentRunner for FakeRunner {
        async fn run(&self, req: SubAgentRequest<'_>) -> Result<String, String> {
            self.captured.lock().unwrap().push(CapturedRequest {
                user_prompt: req.user_prompt.to_string(),
                system_prompt: req.system_prompt.to_string(),
                subtask_id: req.subtask_id.to_string(),
                read_only: req.read_only,
            });
            self.response.clone()
        }
    }

    /// UserQuestionAsker stub — never called from these tests but needed
    /// to populate OrchestrationCtx.
    struct StubAsker;

    #[async_trait]
    impl UserQuestionAsker for StubAsker {
        async fn ask(
            &self,
            _: crate::tools::ask_user_question::UserQuestionRequest<'_>,
        ) -> Result<String, String> {
            unreachable!("StubAsker should not be invoked in StartSubAgent tests")
        }
    }

    #[tokio::test]
    async fn missing_prompt_errors() {
        let tool = StartSubAgentTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("prompt"));
    }

    #[tokio::test]
    async fn empty_prompt_errors() {
        let tool = StartSubAgentTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool.execute(json!({"prompt": "   \n"}), &ctx).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn errors_when_no_orchestration_ctx() {
        let tool = StartSubAgentTool;
        let token = CancellationToken::new();
        let ctx = ToolContext::new(Path::new("/tmp"), false, &token);
        let result = tool
            .execute(json!({"prompt": "do something"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("orchestration"));
    }

    #[test]
    fn derive_label_strips_punctuation_and_kebabs() {
        assert_eq!(derive_label("audit auth.rs for issues"), "audit-auth-rs-for-issues");
        assert_eq!(derive_label("   "), "agent");
        assert_eq!(derive_label("!!"), "agent");
        let long_label = derive_label("a very long prompt about many things");
        assert!(long_label.len() <= DEFAULT_AGENT_LABEL_LEN);
        assert!(!long_label.ends_with('-'));
    }

    #[test]
    fn read_only_input_marks_invocation_read_only() {
        let tool = StartSubAgentTool;
        assert!(tool.is_read_only(&json!({"read_only": true})));
        assert!(!tool.is_read_only(&json!({"read_only": false})));
        assert!(!tool.is_read_only(&json!({})));
    }

    #[test]
    fn is_session_scoped() {
        assert!(matches!(StartSubAgentTool.scope(), ToolScope::Session));
    }

    #[test]
    fn schema_declares_required_prompt() {
        let schema = StartSubAgentTool.input_schema();
        assert_eq!(schema["required"][0], "prompt");
    }

    // Compile-only smoke tests — exercising the orchestration path here
    // requires a Tauri AppHandle & AppConfig, which we can't easily build
    // in unit tests without a runtime. The fakes are kept in scope so
    // future integration tests can use them as-is.
    #[allow(dead_code)]
    fn _fake_runner_compiles() -> FakeRunner {
        FakeRunner {
            response: Ok("ok".to_string()),
            captured: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    fn _stub_asker_compiles() -> StubAsker {
        StubAsker
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
