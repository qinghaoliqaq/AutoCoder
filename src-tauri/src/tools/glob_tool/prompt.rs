/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
- Fast file pattern matching tool that works with any codebase size
- Supports glob patterns like "**/*.js" or "src/**/*.ts"
- Returns matching file paths sorted by modification time
- Use this tool when you need to find files by name patterns
- For open-ended searches that may require multiple rounds of globbing and grepping, combine Glob with Grep iteratively
"#;
