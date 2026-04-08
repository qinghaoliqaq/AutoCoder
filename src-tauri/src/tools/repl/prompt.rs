/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
Execute code in a REPL (Python, Node.js, or Ruby).

Runs the given code snippet as a subprocess and captures stdout/stderr. Use this for quick code
evaluation, testing snippets, or running scripts that don't need file persistence.

Supported languages:
  - "python": Runs via python3 -c
  - "node": Runs via node -e
  - "ruby": Runs via ruby -e

Usage notes:
  - Both `language` and `code` parameters are required.
  - Optional timeout in milliseconds (max 300000ms / 5 minutes, default 30000ms / 30 seconds).
  - The code is executed in the workspace directory.
  - Shell state does not persist between calls.
  - For long-running computations, increase the timeout.
"#;
