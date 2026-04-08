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

/// Generate a simple unique ID based on timestamp and a random component.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Use the lower bits mixed with process id for uniqueness
    let pid = std::process::id();
    format!("{:x}-{:x}", nanos, pid)
}

pub struct TaskCreateTool;

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &'static str {
        "TaskCreate"
    }

    fn description(&self) -> &'static str {
        "Create a new task in the task list. Use this to track multi-step work, \
         organize complex tasks, and show progress to the user. Each task is created \
         with status 'pending' and can later be updated via TaskUpdate."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["description", "prompt"],
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A brief description of what the task does"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt or instructions for the task"
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let description = match input.get("description").and_then(|v| v.as_str()) {
            Some(d) => d.to_string(),
            None => return ToolResult::err("Missing required parameter: description"),
        };
        let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return ToolResult::err("Missing required parameter: prompt"),
        };

        let path = tasks_file(ctx);
        let mut tasks = match read_tasks(&path) {
            Ok(t) => t,
            Err(e) => return ToolResult::err(e),
        };

        let id = generate_id();
        let now = chrono::Utc::now().to_rfc3339();

        let task = json!({
            "id": id,
            "description": description,
            "prompt": prompt,
            "status": "pending",
            "created_at": now,
            "updated_at": now,
            "output": null
        });

        tasks.push(task);

        if let Err(e) = write_tasks(&path, &tasks) {
            return ToolResult::err(e);
        }

        ToolResult::ok(format!("Task created successfully with id: {id}"))
    }
}
