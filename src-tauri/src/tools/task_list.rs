use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::{Tool, ToolContext, ToolResult};

/// Shared helper: path to the tasks file within a workspace.
fn tasks_file(ctx: &ToolContext<'_>) -> PathBuf {
    ctx.workspace.join(".autocoder/tasks.json")
}

/// Read existing tasks from disk.  Returns an empty Vec if the file does not exist.
fn read_tasks(path: &PathBuf) -> Result<Vec<Value>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read tasks file: {e}"))?;
    let arr: Vec<Value> =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse tasks file: {e}"))?;
    Ok(arr)
}

pub struct TaskListTool;

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &'static str {
        "TaskList"
    }

    fn description(&self) -> &'static str {
        "List all tasks in the task list, optionally filtered by status. Returns a \
         summary of each task including id, description, status, and timestamps. Use \
         this to see overall progress, find available work, or check for blocked tasks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "Optional filter by status: 'pending', 'running', 'completed', 'failed', or 'stopped'",
                    "enum": ["pending", "running", "completed", "failed", "stopped"]
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let status_filter = input.get("status").and_then(|v| v.as_str());

        let path = tasks_file(ctx);
        let tasks = match read_tasks(&path) {
            Ok(t) => t,
            Err(e) => return ToolResult::err(e),
        };

        let filtered: Vec<&Value> = if let Some(status) = status_filter {
            tasks
                .iter()
                .filter(|t| t.get("status").and_then(|v| v.as_str()) == Some(status))
                .collect()
        } else {
            tasks.iter().collect()
        };

        if filtered.is_empty() {
            let msg = if let Some(status) = status_filter {
                format!("No tasks found with status: {status}")
            } else {
                "No tasks found".to_string()
            };
            return ToolResult::ok(msg);
        }

        // Build a summary list with key fields
        let summaries: Vec<Value> = filtered
            .iter()
            .map(|t| {
                json!({
                    "id": t.get("id").cloned().unwrap_or(Value::Null),
                    "description": t.get("description").cloned().unwrap_or(Value::Null),
                    "status": t.get("status").cloned().unwrap_or(Value::Null),
                    "created_at": t.get("created_at").cloned().unwrap_or(Value::Null),
                    "updated_at": t.get("updated_at").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();

        let pretty =
            serde_json::to_string_pretty(&summaries).unwrap_or_else(|_| format!("{summaries:?}"));
        ToolResult::ok(pretty)
    }
}
