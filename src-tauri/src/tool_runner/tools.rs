/// Tool schema definitions and read-only detection.
///
/// All 4 tools are 100% local Rust execution — only the JSON schema format
/// differs between Anthropic (built-in shorthand) and OpenAI (standard JSON Schema).

use super::providers::WireFormat;
use serde_json::{json, Value};

// ── Individual tool schemas (OpenAI / universal format) ─────────────────────

pub fn bash_schema() -> Value {
    json!({
        "name": "bash",
        "description": "Execute a shell command and return stdout, stderr, exit code.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" },
                "timeout": { "type": "integer", "description": "Optional timeout in ms (max 600000)" }
            },
            "required": ["command"]
        }
    })
}

pub fn editor_schema() -> Value {
    json!({
        "name": "str_replace_based_edit_tool",
        "description": "Text editor for viewing, creating, and editing files.\nCommands: view, create, str_replace, insert",
        "input_schema": {
            "type": "object",
            "properties": {
                "command":     { "type": "string", "enum": ["view","create","str_replace","insert"], "description": "Editor command" },
                "path":        { "type": "string", "description": "File path" },
                "file_text":   { "type": "string", "description": "Content for create" },
                "old_str":     { "type": "string", "description": "String to find for str_replace (must be unique)" },
                "new_str":     { "type": "string", "description": "Replacement for str_replace or text for insert" },
                "insert_line": { "type": "integer", "description": "Line number for insert" }
            },
            "required": ["command", "path"]
        }
    })
}

pub fn grep_schema() -> Value {
    json!({
        "name": "grep_search",
        "description": "Search for a regex pattern across files. Returns matching lines with paths and line numbers.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern" },
                "path":    { "type": "string", "description": "Directory to search (absolute path)" },
                "include": { "type": "string", "description": "File glob filter (e.g. '*.rs')" }
            },
            "required": ["pattern", "path"]
        }
    })
}

pub fn glob_schema() -> Value {
    json!({
        "name": "glob_find",
        "description": "Find files matching a glob pattern. Returns paths sorted by mtime.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. 'src/**/*.rs')" },
                "path":    { "type": "string", "description": "Root directory (absolute path)" }
            },
            "required": ["pattern", "path"]
        }
    })
}

// ── Assembled tool lists ────────────────────────────────────────────────────

/// Build tool definitions for the given wire format.
pub fn definitions(format: WireFormat) -> Vec<Value> {
    let (bash, editor) = match format {
        WireFormat::Anthropic => (
            json!({ "type": "bash_20250124", "name": "bash" }),
            json!({ "type": "text_editor_20250728", "name": "str_replace_based_edit_tool" }),
        ),
        WireFormat::OpenAI => (bash_schema(), editor_schema()),
    };
    vec![bash, editor, grep_schema(), glob_schema()]
}

/// Convert to OpenAI function-calling wire format.
pub fn to_openai_functions(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["input_schema"]
                }
            })
        })
        .collect()
}

// ── Read-only detection ─────────────────────────────────────────────────────

/// Returns true if the tool call is read-only and safe to run concurrently.
pub fn is_read_only(name: &str, input: &Value) -> bool {
    match name {
        "grep_search" | "glob_find" => true,
        "str_replace_based_edit_tool" => input["command"].as_str() == Some("view"),
        "bash" => {
            if let Some(cmd) = input["command"].as_str() {
                let t = cmd.trim();
                READ_ONLY_PREFIXES.iter().any(|p| t.starts_with(p))
            } else {
                false
            }
        }
        _ => false,
    }
}

const READ_ONLY_PREFIXES: &[&str] = &[
    "cat ", "head ", "tail ", "less ", "wc ", "file ", "ls ", "ls\n",
    "pwd", "echo ", "which ", "type ", "find ", "grep ", "rg ", "ag ",
    "fd ", "git log", "git show", "git diff", "git status",
    "git branch", "git rev-parse", "git remote", "cargo check",
    "cargo clippy", "rustc --", "python -c", "node -e", "stat ",
    "du ", "df ",
];

/// Summarize tool input for the frontend tool-log event.
pub fn summarize_input(name: &str, input: &Value) -> String {
    match name {
        "bash" => input["command"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(150)
            .collect(),
        "str_replace_based_edit_tool" => {
            let cmd = input["command"].as_str().unwrap_or("");
            let path = input["path"].as_str().unwrap_or("");
            format!("{cmd} {path}")
        }
        "grep_search" => {
            let pattern = input["pattern"].as_str().unwrap_or("");
            let path = input["path"].as_str().unwrap_or("");
            format!("/{pattern}/ in {path}")
        }
        "glob_find" => {
            let pattern = input["pattern"].as_str().unwrap_or("");
            format!("find {pattern}")
        }
        _ => serde_json::to_string(input)
            .unwrap_or_default()
            .chars()
            .take(150)
            .collect(),
    }
}
