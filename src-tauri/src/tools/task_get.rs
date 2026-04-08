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

pub struct TaskGetTool;

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &'static str {
        "TaskGet"
    }

    fn description(&self) -> &'static str {
        "Get a task by ID from the task list. Returns full task details including \
         description, prompt, status, timestamps, and output. Use this to retrieve \
         complete information about a specific task before starting work on it or \
         checking its current state."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to retrieve"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let task_id = match input.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("Missing required parameter: task_id"),
        };

        let path = tasks_file(ctx);
        let tasks = match read_tasks(&path) {
            Ok(t) => t,
            Err(e) => return ToolResult::err(e),
        };

        let task = tasks
            .iter()
            .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(task_id));

        match task {
            Some(t) => {
                let pretty = serde_json::to_string_pretty(t).unwrap_or_else(|_| t.to_string());
                ToolResult::ok(pretty)
            }
            None => ToolResult::err(format!("Task not found: {task_id}")),
        }
    }
}
