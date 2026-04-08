use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// Agent tool — launches a new agent to handle complex, multi-step tasks autonomously.
///
/// This is a stub: actual agent spawning requires integration with the orchestration
/// layer (tool_runner loop, sub-agent context, cancellation propagation, etc.).
pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &'static str {
        "Agent"
    }

    fn description(&self) -> &'static str {
        "Launch a new agent to handle complex, multi-step tasks autonomously. \
         The Agent tool launches specialized agents (subprocesses) that autonomously \
         handle complex tasks. Each agent type has specific capabilities and tools \
         available to it. When using the Agent tool, specify a subagent_type parameter \
         to select which agent type to use. If omitted, the general-purpose agent is used."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["prompt", "description"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform. Brief the agent like a smart colleague who just walked into the room — explain what you're trying to accomplish, what you've already learned, and give enough context for judgment calls."
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task. If omitted, the general-purpose agent is used."
                },
                "model": {
                    "type": "string",
                    "enum": ["sonnet", "opus", "haiku"],
                    "description": "Optional model override for this agent. If omitted, uses the agent definition's model or inherits from the parent."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this agent in the background. You will be notified when it completes."
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree"],
                    "description": "Isolation mode. \"worktree\" creates a temporary git worktree so the agent works on an isolated copy of the repo."
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let prompt = input["prompt"].as_str().unwrap_or("");
        let description = input["description"].as_str().unwrap_or("(no description)");
        let subagent_type = input["subagent_type"].as_str();

        if prompt.is_empty() {
            return ToolResult::err("Missing required parameter: prompt");
        }

        let agent_info = match subagent_type {
            Some(t) => format!(" (subagent_type: {t})"),
            None => String::new(),
        };

        ToolResult::ok(format!(
            "Agent sub-task spawning is handled by the orchestration layer. \
             Describe the task in `prompt` and the system will route it.\n\
             Task: {description}{agent_info}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn agent_tool_returns_stub() {
        let tool = AgentTool;
        assert_eq!(tool.name(), "Agent");
        assert!(!tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(
                json!({"prompt": "do something", "description": "test task"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("orchestration layer"));
    }

    #[tokio::test]
    async fn agent_tool_missing_prompt() {
        let tool = AgentTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"description": "test"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("prompt"));
    }
}
