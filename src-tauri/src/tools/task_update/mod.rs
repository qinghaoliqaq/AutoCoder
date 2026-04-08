pub mod prompt;

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

/// Write tasks back to disk, creating the parent directory if needed.
fn write_tasks(path: &PathBuf, tasks: &[Value]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .autocoder directory: {e}"))?;
    }
    let data = serde_json::to_string_pretty(tasks)
        .map_err(|e| format!("Failed to serialize tasks: {e}"))?;
    std::fs::write(path, data).map_err(|e| format!("Failed to write tasks file: {e}"))?;
    Ok(())
}

/// Valid task statuses.
const VALID_STATUSES: &[&str] = &["pending", "running", "completed", "failed", "stopped"];

pub struct TaskUpdateTool;

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &'static str {
        "TaskUpdate"
    }

    fn description(&self) -> &'static str {
        "Update a task in the task list. Can change status and/or output. Use this to \
         mark tasks as running when you start work, completed when finished, or failed \
         if something went wrong. You can also attach output text to record results."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to update"
                },
                "status": {
                    "type": "string",
                    "description": "New status for the task",
                    "enum": ["pending", "running", "completed", "failed", "stopped"]
                },
                "output": {
                    "type": "string",
                    "description": "Output or result text to attach to the task"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let task_id = match input.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("Missing required parameter: task_id"),
        };

        let new_status = input.get("status").and_then(|v| v.as_str());
        let new_output = input.get("output").and_then(|v| v.as_str());

        // Validate status if provided
        if let Some(status) = new_status {
            if !VALID_STATUSES.contains(&status) {
                return ToolResult::err(format!(
                    "Invalid status: '{status}'. Must be one of: {}",
                    VALID_STATUSES.join(", ")
                ));
            }
        }

        // At least one field must be provided
        if new_status.is_none() && new_output.is_none() {
            return ToolResult::err(
                "At least one of 'status' or 'output' must be provided to update a task",
            );
        }

        let path = tasks_file(ctx);
        let mut tasks = match read_tasks(&path) {
            Ok(t) => t,
            Err(e) => return ToolResult::err(e),
        };

        let task = tasks
            .iter_mut()
            .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(task_id));

        let task = match task {
            Some(t) => t,
            None => return ToolResult::err(format!("Task not found: {task_id}")),
        };

        let now = chrono::Utc::now().to_rfc3339();
        let mut updated_fields: Vec<&str> = Vec::new();

        if let Some(status) = new_status {
            task["status"] = Value::String(status.to_string());
            updated_fields.push("status");
        }
        if let Some(output) = new_output {
            task["output"] = Value::String(output.to_string());
            updated_fields.push("output");
        }
        task["updated_at"] = Value::String(now);

        if let Err(e) = write_tasks(&path, &tasks) {
            return ToolResult::err(e);
        }

        ToolResult::ok(format!(
            "Task {task_id} updated successfully. Updated fields: {}",
            updated_fields.join(", ")
        ))
    }
}
