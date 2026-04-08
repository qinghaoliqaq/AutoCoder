/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
Execute a skill within the main conversation

When users ask you to perform tasks, check if any of the available skills match. Skills provide specialized capabilities and domain knowledge.

When users reference a "slash command" or "/<something>" (e.g., "/simplify", "/verify"), they are referring to a skill. Use this tool to invoke it.

How to invoke:
- Use this tool with the skill name and optional arguments
- Examples:
  - `skill: "simplify"` - review code for reuse, quality, and efficiency
  - `skill: "verify"` - verify code changes end-to-end
  - `skill: "frontend-dev"` - invoke the frontend dev skill
  - `skill: "verify", args: "check the login flow"` - invoke with arguments

Important:
- When a skill matches the user's request, invoke the Skill tool BEFORE generating any other response about the task
- NEVER mention a skill without actually calling this tool
- Do not invoke a skill that is already running
- If you see a <command-name> tag in the current conversation turn, the skill has ALREADY been loaded - follow the instructions directly instead of calling this tool again
"#;
