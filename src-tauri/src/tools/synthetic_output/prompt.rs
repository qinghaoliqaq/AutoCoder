/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
Use this tool to return your final response in the requested structured format. You MUST call this tool exactly once at the end of your response to provide the structured output.
"#;
