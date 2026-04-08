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

pub struct TaskOutputTool;

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &'static str {
        "TaskOutput"
    }

    fn description(&self) -> &'static str {
        "Get the output of a task by its ID. Returns just the output field of the task, \
         which contains the result or any recorded output text. Use this to retrieve the \
         results of a completed task without fetching all task metadata."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to get output from"
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

        let task = match task {
            Some(t) => t,
            None => return ToolResult::err(format!("Task not found: {task_id}")),
        };

        let status = task
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match task.get("output") {
            Some(Value::String(output)) => {
                ToolResult::ok(format!("Task {task_id} (status: {status}):\n{output}"))
            }
            Some(Value::Null) | None => ToolResult::ok(format!(
                "Task {task_id} (status: {status}): no output recorded"
            )),
            Some(other) => {
                // Handle non-string output values by serializing them
                let output_str =
                    serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string());
                ToolResult::ok(format!("Task {task_id} (status: {status}):\n{output_str}"))
            }
        }
    }
}
