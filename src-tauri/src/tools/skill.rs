use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// Skill tool — executes a skill (slash command) within the main conversation.
///
/// Stub: actual skill invocation is handled by the orchestration layer which
/// loads the skill prompt, runs it in a forked agent context, and returns
/// the result.
pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &'static str {
        "Skill"
    }

    fn description(&self) -> &'static str {
        "Execute a skill within the main conversation. When users ask you to \
         perform tasks, check if any of the available skills match. Skills provide \
         specialized capabilities and domain knowledge. When users reference a \
         \"slash command\" or \"/<something>\" (e.g., \"/commit\", \"/review-pr\"), \
         they are referring to a skill. Use this tool to invoke it."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["skill"],
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"commit\", \"review-pr\", or \"pdf\""
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let skill = match input["skill"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::err("Missing required parameter: skill"),
        };

        let args_info = match input["args"].as_str() {
            Some(a) if !a.is_empty() => format!(" with args: {a}"),
            _ => String::new(),
        };

        // Stub: actual skill invocation is handled by the orchestration layer.
        // In the real system, this would:
        // 1. Look up the skill by name in the command registry
        // 2. Load its prompt and configuration
        // 3. Run it in a forked agent context
        // 4. Return the skill's output
        ToolResult::ok(format!(
            "Skill invocation is handled by the orchestration layer. \
             Requested skill: {skill}{args_info}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn skill_returns_stub() {
        let tool = SkillTool;
        assert_eq!(tool.name(), "Skill");
        assert!(!tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"skill": "commit", "args": "-m 'Fix bug'"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("commit"));
        assert!(result.content.contains("Fix bug"));
    }

    #[tokio::test]
    async fn skill_without_args() {
        let tool = SkillTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool
            .execute(json!({"skill": "review-pr"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("review-pr"));
        assert!(!result.content.contains("with args"));
    }

    #[tokio::test]
    async fn skill_missing_name() {
        let tool = SkillTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("skill"));
    }
}
