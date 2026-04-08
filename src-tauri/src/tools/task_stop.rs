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

pub struct TaskStopTool;

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &'static str {
        "TaskStop"
    }

    fn description(&self) -> &'static str {
        "Stop a running task by setting its status to 'stopped'. Use this when you need \
         to terminate a long-running or stuck task. The task's status will be changed to \
         'stopped' and the updated_at timestamp will be refreshed."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the task to stop"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let task_id = match input.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("Missing required parameter: task_id"),
        };

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

        let current_status = task
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Only allow stopping tasks that are in a running or pending state
        if current_status != "running" && current_status != "pending" {
            return ToolResult::err(format!(
                "Cannot stop task {task_id}: current status is '{current_status}'. \
                 Only 'pending' or 'running' tasks can be stopped."
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        task["status"] = Value::String("stopped".to_string());
        task["updated_at"] = Value::String(now);

        if let Err(e) = write_tasks(&path, &tasks) {
            return ToolResult::err(e);
        }

        ToolResult::ok(format!(
            "Task {task_id} stopped successfully (was '{current_status}')"
        ))
    }
}
