pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};
use crate::bundled_skills;

/// Skill tool — executes a bundled skill by name.
///
/// Skills are prompt-based contextual guides that provide domain-specific
/// instructions for the coding agent. When invoked, the skill's full prompt
/// is returned so the model can follow the specialized instructions.
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
         \"slash command\" or \"/<something>\" (e.g., \"/simplify\", \"/verify\"), \
         they are referring to a skill. Use this tool to invoke it."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["skill"],
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"simplify\", \"verify\", \"frontend-dev\""
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
        // SkillTool.execute only looks up a bundled skill by name and
        // returns its prompt text — no filesystem writes, no shell, no
        // network.  Follow-up tool calls the model makes based on that
        // prompt are gated by their own is_read_only checks.
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let skill_name = match input["skill"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::err("Missing required parameter: skill"),
        };

        let registry = bundled_skills::default_skill_registry();

        // Try exact match first, then try with common variations
        let skill = registry
            .get(skill_name)
            .or_else(|| registry.get(&skill_name.replace('_', "-")))
            .or_else(|| registry.get(&skill_name.to_lowercase()));

        match skill {
            Some(def) => {
                let args_note = match input["args"].as_str() {
                    Some(a) if !a.is_empty() => format!("\n\nUser arguments: {a}"),
                    _ => String::new(),
                };
                ToolResult::ok(format!(
                    "# Skill: {}\n\n{}{args_note}",
                    def.label, def.prompt
                ))
            }
            None => {
                // List available skills
                let available: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|(slug, label, desc, _)| format!("  - /{slug} — {label}: {desc}"))
                    .collect();
                ToolResult::err(format!(
                    "Unknown skill: {skill_name}\n\nAvailable skills:\n{}",
                    available.join("\n")
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn ctx() -> (CancellationToken, impl Fn() -> ToolContext<'static>) {
        let token = CancellationToken::new();
        let token_ref = token.clone();
        let make_ctx = move || ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: Box::leak(Box::new(token_ref.clone())),
        };
        (token, make_ctx)
    }

    #[tokio::test]
    async fn invoke_known_skill() {
        let tool = SkillTool;
        let (_token, make_ctx) = ctx();
        let result = tool
            .execute(json!({"skill": "simplify"}), &make_ctx())
            .await;
        assert!(!result.is_error, "error: {}", result.content);
        assert!(result.content.contains("Simplify"));
        assert!(result.content.contains("Code Reuse"));
    }

    #[tokio::test]
    async fn invoke_with_args() {
        let tool = SkillTool;
        let (_token, make_ctx) = ctx();
        let result = tool
            .execute(
                json!({"skill": "verify", "args": "check the login flow"}),
                &make_ctx(),
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Verify"));
        assert!(result.content.contains("check the login flow"));
    }

    #[tokio::test]
    async fn unknown_skill_lists_available() {
        let tool = SkillTool;
        let (_token, make_ctx) = ctx();
        let result = tool
            .execute(json!({"skill": "nonexistent"}), &make_ctx())
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("Unknown skill"));
        assert!(result.content.contains("/simplify"));
        assert!(result.content.contains("/frontend-dev"));
    }

    #[tokio::test]
    async fn missing_skill_name() {
        let tool = SkillTool;
        let (_token, make_ctx) = ctx();
        let result = tool.execute(json!({}), &make_ctx()).await;
        assert!(result.is_error);
        assert!(result.content.contains("skill"));
    }
}
