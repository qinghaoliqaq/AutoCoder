/// ScheduleCronTool — manage scheduled tasks (cron jobs).
///
/// Stores task definitions in `.autocoder/cron.json` in the workspace.
/// Supports create, delete, and list actions.
use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct ScheduleCronTool;

const CRON_DIR: &str = ".autocoder";
const CRON_FILE: &str = "cron.json";

/// Resolve the cron file path within the workspace.
fn cron_path(workspace: &std::path::Path) -> PathBuf {
    workspace.join(CRON_DIR).join(CRON_FILE)
}

/// Read the cron file, returning an empty array if it doesn't exist.
fn read_cron_entries(workspace: &std::path::Path) -> Result<Vec<Value>, String> {
    let path = cron_path(workspace);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read cron file: {e}"))?;
    let entries: Vec<Value> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse cron file: {e}"))?;
    Ok(entries)
}

/// Write the cron entries back to the file, creating directories as needed.
fn write_cron_entries(workspace: &std::path::Path, entries: &[Value]) -> Result<(), String> {
    let path = cron_path(workspace);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cron directory: {e}"))?;
    }
    let content = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("Failed to serialize cron entries: {e}"))?;
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write cron file: {e}"))?;
    Ok(())
}

/// Generate a simple unique ID for a cron entry.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("cron_{ts}")
}

const SCHEDULE_CRON_DESCRIPTION: &str = "Manage scheduled tasks (cron jobs). \
Stores schedules in `.autocoder/cron.json` in the workspace. \
Supports actions: \"create\" (add a new scheduled task), \"delete\" (remove by name), \"list\" (show all tasks).";

#[async_trait]
impl Tool for ScheduleCronTool {
    fn name(&self) -> &'static str {
        "ScheduleCron"
    }

    fn description(&self) -> &'static str {
        SCHEDULE_CRON_DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "delete", "list"],
                    "description": "The cron action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Name/identifier for the scheduled task (required for create and delete)"
                },
                "schedule": {
                    "type": "string",
                    "description": "Cron expression (e.g. '*/5 * * * *' for every 5 minutes). Required for create."
                },
                "command": {
                    "type": "string",
                    "description": "The command or prompt to run on schedule. Required for create."
                }
            }
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        matches!(input["action"].as_str(), Some("list"))
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let action = match input["action"].as_str() {
            Some(a) if !a.is_empty() => a,
            _ => return ToolResult::err("Missing or empty 'action' parameter"),
        };

        match action {
            "list" => {
                let entries = match read_cron_entries(ctx.workspace) {
                    Ok(e) => e,
                    Err(e) => return ToolResult::err(e),
                };
                if entries.is_empty() {
                    return ToolResult::ok("No scheduled tasks.");
                }
                let mut lines = Vec::new();
                for entry in &entries {
                    let name = entry["name"].as_str().unwrap_or("(unnamed)");
                    let schedule = entry["schedule"].as_str().unwrap_or("(no schedule)");
                    let command = entry["command"].as_str().unwrap_or("(no command)");
                    lines.push(format!("  {name} — {schedule} — {command}"));
                }
                ToolResult::ok(format!(
                    "Scheduled tasks ({}):\n{}",
                    entries.len(),
                    lines.join("\n")
                ))
            }
            "create" => {
                let name = match input["name"].as_str() {
                    Some(n) if !n.trim().is_empty() => n,
                    _ => return ToolResult::err("Missing or empty 'name' parameter for 'create' action"),
                };
                let schedule = match input["schedule"].as_str() {
                    Some(s) if !s.trim().is_empty() => s,
                    _ => return ToolResult::err("Missing or empty 'schedule' parameter for 'create' action"),
                };
                let command = match input["command"].as_str() {
                    Some(c) if !c.trim().is_empty() => c,
                    _ => return ToolResult::err("Missing or empty 'command' parameter for 'create' action"),
                };

                let mut entries = match read_cron_entries(ctx.workspace) {
                    Ok(e) => e,
                    Err(e) => return ToolResult::err(e),
                };

                // Check for duplicate name
                if entries.iter().any(|e| e["name"].as_str() == Some(name)) {
                    return ToolResult::err(format!(
                        "A scheduled task with name '{name}' already exists. Delete it first or use a different name."
                    ));
                }

                let id = generate_id();
                let entry = json!({
                    "id": id,
                    "name": name,
                    "schedule": schedule,
                    "command": command,
                });
                entries.push(entry);

                if let Err(e) = write_cron_entries(ctx.workspace, &entries) {
                    return ToolResult::err(e);
                }

                ToolResult::ok(format!(
                    "Created scheduled task '{name}' (id: {id})\n  Schedule: {schedule}\n  Command: {command}"
                ))
            }
            "delete" => {
                let name = match input["name"].as_str() {
                    Some(n) if !n.trim().is_empty() => n,
                    _ => return ToolResult::err("Missing or empty 'name' parameter for 'delete' action"),
                };

                let mut entries = match read_cron_entries(ctx.workspace) {
                    Ok(e) => e,
                    Err(e) => return ToolResult::err(e),
                };

                let original_len = entries.len();
                entries.retain(|e| e["name"].as_str() != Some(name));

                if entries.len() == original_len {
                    return ToolResult::err(format!(
                        "No scheduled task found with name '{name}'"
                    ));
                }

                if let Err(e) = write_cron_entries(ctx.workspace, &entries) {
                    return ToolResult::err(e);
                }

                ToolResult::ok(format!("Deleted scheduled task '{name}'"))
            }
            other => ToolResult::err(format!(
                "Unknown action: '{other}'. Supported: create, delete, list"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn make_ctx(workspace: &Path) -> ToolContext<'_> {
        let token = Box::leak(Box::new(CancellationToken::new()));
        ToolContext {
            workspace,
            read_only: false,
            token,
        }
    }

    #[test]
    fn test_metadata() {
        let tool = ScheduleCronTool;
        assert_eq!(tool.name(), "ScheduleCron");
        assert!(tool.is_read_only(&json!({"action": "list"})));
        assert!(!tool.is_read_only(&json!({"action": "create"})));
        assert!(!tool.is_read_only(&json!({"action": "delete"})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[tokio::test]
    async fn test_list_empty() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ScheduleCronTool
            .execute(json!({"action": "list"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("No scheduled tasks"));
    }

    #[tokio::test]
    async fn test_create_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);

        // Create a task
        let result = ScheduleCronTool
            .execute(
                json!({
                    "action": "create",
                    "name": "daily-backup",
                    "schedule": "0 2 * * *",
                    "command": "backup.sh"
                }),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("daily-backup"));
        assert!(result.content.contains("0 2 * * *"));

        // List tasks
        let result = ScheduleCronTool
            .execute(json!({"action": "list"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("daily-backup"));
        assert!(result.content.contains("1"));
    }

    #[tokio::test]
    async fn test_create_duplicate_name() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);

        // Create first
        ScheduleCronTool
            .execute(
                json!({
                    "action": "create",
                    "name": "test-job",
                    "schedule": "*/5 * * * *",
                    "command": "echo hello"
                }),
                &ctx,
            )
            .await;

        // Create duplicate
        let result = ScheduleCronTool
            .execute(
                json!({
                    "action": "create",
                    "name": "test-job",
                    "schedule": "*/10 * * * *",
                    "command": "echo world"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("already exists"));
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);

        // Create
        ScheduleCronTool
            .execute(
                json!({
                    "action": "create",
                    "name": "to-delete",
                    "schedule": "0 * * * *",
                    "command": "cleanup.sh"
                }),
                &ctx,
            )
            .await;

        // Delete
        let result = ScheduleCronTool
            .execute(json!({"action": "delete", "name": "to-delete"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Deleted"));

        // Verify empty
        let result = ScheduleCronTool
            .execute(json!({"action": "list"}), &ctx)
            .await;
        assert!(result.content.contains("No scheduled tasks"));
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ScheduleCronTool
            .execute(json!({"action": "delete", "name": "ghost"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("No scheduled task found"));
    }

    #[tokio::test]
    async fn test_missing_action() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ScheduleCronTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("action"));
    }
}
