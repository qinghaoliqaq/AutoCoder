pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct LSPTool;

#[async_trait]
impl Tool for LSPTool {
    fn name(&self) -> &'static str {
        "LSP"
    }

    fn description(&self) -> &'static str {
        "Interact with Language Server Protocol (LSP) servers to get code intelligence features.\n\
         \n\
         Supported actions:\n\
         - diagnostics: Get diagnostic messages (errors, warnings) for a file\n\
         - hover: Get hover information (documentation, type info) for a symbol at a position\n\
         - definition: Go to the definition of a symbol at a position\n\
         - references: Find all references to a symbol at a position\n\
         \n\
         All actions require:\n\
         - file_path: The absolute path to the file to operate on\n\
         \n\
         Position-based actions (hover, definition, references) also require:\n\
         - line: The line number (1-based, as shown in editors)\n\
         - character: The character offset (1-based, as shown in editors)\n\
         \n\
         Note: LSP servers must be configured and running for the file type.\n\
         If no server is available, an informative error will be returned."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["diagnostics", "hover", "definition", "references"],
                    "description": "The LSP action to perform"
                },
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to operate on"
                },
                "line": {
                    "type": "integer",
                    "description": "The line number (1-based). Required for hover, definition, and references actions."
                },
                "character": {
                    "type": "integer",
                    "description": "The character offset (1-based). Required for hover, definition, and references actions."
                }
            },
            "required": ["action", "file_path"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        // Validate input parameters
        let action = match input.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing required parameter: action"),
        };

        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };

        // Validate action
        if action != "diagnostics" && action != "hover" && action != "definition" && action != "references" {
            return ToolResult::err(format!(
                "Invalid action: '{action}'. Must be one of: diagnostics, hover, definition, references"
            ));
        }

        // For position-based actions, validate line and character are present
        if action != "diagnostics" {
            if input.get("line").and_then(|v| v.as_i64()).is_none() {
                return ToolResult::err(format!(
                    "Missing required parameter: line (required for '{action}' action)"
                ));
            }
            if input.get("character").and_then(|v| v.as_i64()).is_none() {
                return ToolResult::err(format!(
                    "Missing required parameter: character (required for '{action}' action)"
                ));
            }
        }

        // Acknowledge parameters to avoid unused warnings
        let _ = file_path;

        // Stub: LSP integration not yet available
        ToolResult::err(
            "LSP integration not yet available. \
             Use Grep and Read tools to navigate code.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_lsp() {
        let tool = LSPTool;
        assert_eq!(tool.name(), "LSP");
    }

    #[test]
    fn is_always_read_only() {
        let tool = LSPTool;
        assert!(tool.is_read_only(&json!({})));
        assert!(tool.is_read_only(&json!({"action": "hover"})));
    }

    #[test]
    fn schema_has_required_fields() {
        let tool = LSPTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("file_path")));
    }

    #[test]
    fn schema_action_enum() {
        let tool = LSPTool;
        let schema = tool.input_schema();
        let action_enum = schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        assert!(action_enum.contains(&json!("diagnostics")));
        assert!(action_enum.contains(&json!("hover")));
        assert!(action_enum.contains(&json!("definition")));
        assert!(action_enum.contains(&json!("references")));
    }

    #[test]
    fn schema_has_position_fields() {
        let tool = LSPTool;
        let schema = tool.input_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("line"));
        assert!(props.contains_key("character"));
    }
}
