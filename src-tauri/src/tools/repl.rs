/// REPLTool — executes code in a language REPL (Python, Node.js, or Ruby).
///
/// Runs the given code snippet as a subprocess and returns stdout/stderr.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

pub struct REPLTool;

/// Maximum timeout in milliseconds (5 minutes).
const MAX_TIMEOUT_MS: u64 = 300_000;
/// Default timeout in milliseconds (30 seconds).
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Supported languages and their corresponding interpreter commands.
const SUPPORTED_LANGUAGES: &[(&str, &[&str])] = &[
    ("python", &["python3", "-c"]),
    ("node", &["node", "-e"]),
    ("ruby", &["ruby", "-e"]),
];

fn interpreter_for_language(language: &str) -> Option<(&'static str, &'static str)> {
    let lower = language.to_lowercase();
    for &(lang, args) in SUPPORTED_LANGUAGES {
        if lower == lang {
            return Some((args[0], args[1]));
        }
    }
    None
}

const REPL_DESCRIPTION: &str = r#"Execute code in a REPL (Python, Node.js, or Ruby).

Runs the given code snippet as a subprocess and captures stdout/stderr. Use this for quick code evaluation, testing snippets, or running scripts that don't need file persistence.

Supported languages:
  - "python": Runs via python3 -c
  - "node": Runs via node -e
  - "ruby": Runs via ruby -e

Usage notes:
  - Both `language` and `code` parameters are required.
  - Optional timeout in milliseconds (max 300000ms / 5 minutes, default 30000ms / 30 seconds).
  - The code is executed in the workspace directory.
  - Shell state does not persist between calls.
  - For long-running computations, increase the timeout."#;

#[async_trait]
impl Tool for REPLTool {
    fn name(&self) -> &'static str {
        "REPL"
    }

    fn description(&self) -> &'static str {
        REPL_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["language", "code"],
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "node", "ruby"],
                    "description": "The language runtime to use"
                },
                "code": {
                    "type": "string",
                    "description": "The code to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 300000, default 30000)"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        // Code execution can have side effects, so never read-only
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let language = match input["language"].as_str() {
            Some(lang) if !lang.trim().is_empty() => lang,
            _ => return ToolResult::err("Missing or empty 'language' parameter"),
        };

        let code = match input["code"].as_str() {
            Some(c) if !c.trim().is_empty() => c,
            _ => return ToolResult::err("Missing or empty 'code' parameter"),
        };

        let (interpreter, flag) = match interpreter_for_language(language) {
            Some(pair) => pair,
            None => {
                return ToolResult::err(format!(
                    "Unsupported language: '{language}'. Supported: python, node, ruby"
                ))
            }
        };

        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let workspace = ctx.workspace.to_path_buf();
        let timeout_duration = Duration::from_millis(timeout_ms);

        let child_future = Command::new(interpreter)
            .arg(flag)
            .arg(code)
            .current_dir(&workspace)
            .output();

        let result = tokio::select! {
            _ = ctx.token.cancelled() => {
                return ToolResult::err("REPL execution cancelled");
            }
            result = tokio::time::timeout(timeout_duration, child_future) => {
                result
            }
        };

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut result_parts = Vec::new();

                if !stdout.is_empty() {
                    result_parts.push(stdout.to_string());
                }

                if !stderr.is_empty() {
                    if !result_parts.is_empty() {
                        result_parts.push(String::new());
                    }
                    result_parts.push(format!("stderr:\n{stderr}"));
                }

                if exit_code != 0 {
                    result_parts.push(format!("\nExit code: {exit_code}"));
                }

                if result_parts.is_empty() {
                    result_parts.push(format!("(no output, exit code {exit_code})"));
                }

                let content = result_parts.join("\n");

                if exit_code == 0 {
                    ToolResult::ok(content)
                } else {
                    ToolResult::err(content)
                }
            }
            Ok(Err(e)) => ToolResult::err(format!(
                "Failed to execute {language} ({interpreter}): {e}"
            )),
            Err(_) => ToolResult::err(format!(
                "REPL execution timed out after {timeout_ms}ms. Consider increasing the timeout."
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpreter_lookup() {
        assert_eq!(
            interpreter_for_language("python"),
            Some(("python3", "-c"))
        );
        assert_eq!(interpreter_for_language("node"), Some(("node", "-e")));
        assert_eq!(interpreter_for_language("ruby"), Some(("ruby", "-e")));
        assert_eq!(
            interpreter_for_language("Python"),
            Some(("python3", "-c"))
        );
        assert_eq!(interpreter_for_language("rust"), None);
        assert_eq!(interpreter_for_language(""), None);
    }

    #[test]
    fn test_metadata() {
        let tool = REPLTool;
        assert_eq!(tool.name(), "REPL");
        assert!(!tool.is_read_only(&json!({"language": "python", "code": "print(1)"})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[test]
    fn test_schema_has_required_fields() {
        let tool = REPLTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("language")));
        assert!(required.contains(&json!("code")));
    }
}
