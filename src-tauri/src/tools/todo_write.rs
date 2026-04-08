use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// TodoWrite tool — creates and manages a structured task list for the current
/// coding session. Writes the todo list to `.autocoder/todos.json` in the workspace.
pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &'static str {
        "TodoWrite"
    }

    fn description(&self) -> &'static str {
        "Update the todo list for the current session. To be used proactively and \
         often to track progress and pending tasks. Make sure that at least one task \
         is in_progress at all times. Always provide both content (imperative) and \
         activeForm (present continuous) for each task."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "required": ["content", "status", "activeForm"],
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "The imperative form describing what needs to be done (e.g., \"Run tests\", \"Build the project\")",
                                "minLength": 1
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status of this task"
                            },
                            "activeForm": {
                                "type": "string",
                                "description": "The present continuous form shown during execution (e.g., \"Running tests\", \"Building the project\")",
                                "minLength": 1
                            }
                        },
                        "additionalProperties": false
                    }
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let todos = match input["todos"].as_array() {
            Some(arr) => arr,
            None => {
                return ToolResult::err(
                    "Missing or invalid required parameter: todos (must be an array)",
                );
            }
        };

        // Validate each todo item
        for (i, todo) in todos.iter().enumerate() {
            let content = todo["content"].as_str().unwrap_or("");
            let status = todo["status"].as_str().unwrap_or("");
            let active_form = todo["activeForm"].as_str().unwrap_or("");

            if content.is_empty() {
                return ToolResult::err(format!(
                    "Todo item {i}: content cannot be empty"
                ));
            }
            if active_form.is_empty() {
                return ToolResult::err(format!(
                    "Todo item {i}: activeForm cannot be empty"
                ));
            }
            if !matches!(status, "pending" | "in_progress" | "completed") {
                return ToolResult::err(format!(
                    "Todo item {i}: status must be one of: pending, in_progress, completed (got: \"{status}\")"
                ));
            }
        }

        // Write todos to .autocoder/todos.json in the workspace
        let autocoder_dir = ctx.workspace.join(".autocoder");
        if let Err(e) = tokio::fs::create_dir_all(&autocoder_dir).await {
            return ToolResult::err(format!(
                "Failed to create .autocoder directory: {e}"
            ));
        }

        let todos_path = autocoder_dir.join("todos.json");
        let todos_json = match serde_json::to_string_pretty(&input["todos"]) {
            Ok(j) => j,
            Err(e) => {
                return ToolResult::err(format!(
                    "Failed to serialize todos: {e}"
                ));
            }
        };

        if let Err(e) = tokio::fs::write(&todos_path, &todos_json).await {
            return ToolResult::err(format!(
                "Failed to write todos to {}: {e}",
                todos_path.display()
            ));
        }

        let count = todos.len();
        ToolResult::ok(format!(
            "Todos have been modified successfully ({count} items written to {}). \
             Ensure that you continue to use the todo list to track your progress. \
             Please proceed with the current tasks if applicable.",
            todos_path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn todo_write_creates_file() {
        let tool = TodoWriteTool;
        assert_eq!(tool.name(), "TodoWrite");
        assert!(!tool.is_read_only(&json!({})));

        let tmp = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: tmp.path(),
            read_only: false,
            token: &token,
        };

        let input = json!({
            "todos": [
                {"content": "Fix bug", "status": "in_progress", "activeForm": "Fixing bug"},
                {"content": "Write tests", "status": "pending", "activeForm": "Writing tests"}
            ]
        });

        let result = tool.execute(input, &ctx).await;
        assert!(!result.is_error, "Error: {}", result.content);
        assert!(result.content.contains("2 items"));

        // Verify file was written
        let todos_path = tmp.path().join(".autocoder").join("todos.json");
        assert!(todos_path.exists());
        let contents = std::fs::read_to_string(&todos_path).unwrap();
        let parsed: Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn todo_write_invalid_status() {
        let tool = TodoWriteTool;
        let tmp = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: tmp.path(),
            read_only: false,
            token: &token,
        };

        let input = json!({
            "todos": [
                {"content": "Fix bug", "status": "unknown", "activeForm": "Fixing bug"}
            ]
        });

        let result = tool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("status"));
    }

    #[tokio::test]
    async fn todo_write_empty_content() {
        let tool = TodoWriteTool;
        let tmp = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: tmp.path(),
            read_only: false,
            token: &token,
        };

        let input = json!({
            "todos": [
                {"content": "", "status": "pending", "activeForm": "Doing something"}
            ]
        });

        let result = tool.execute(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("content"));
    }

    #[tokio::test]
    async fn todo_write_missing_todos() {
        let tool = TodoWriteTool;
        let token = CancellationToken::new();
        let ctx = ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token: &token,
        };

        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
    }
}
