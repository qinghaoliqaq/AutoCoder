/// ConfigTool — view or modify configuration settings.
///
/// Reads and writes `.autocoder/config.json` in the workspace directory.
/// Supports get, set, and list actions.
pub mod prompt;

use super::{Tool, ToolContext, ToolResult, ToolScope};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct ConfigTool;

const CONFIG_DIR: &str = ".autocoder";
const CONFIG_FILE: &str = "config.json";

/// Resolve the config file path within the workspace.
fn config_path(workspace: &std::path::Path) -> PathBuf {
    workspace.join(CONFIG_DIR).join(CONFIG_FILE)
}

/// Read the config file, returning an empty object if it doesn't exist.
/// Uses spawn_blocking to avoid blocking the Tokio runtime on slow filesystems.
async fn read_config(workspace: &std::path::Path) -> Result<Value, String> {
    let path = config_path(workspace);
    if !path.exists() {
        return Ok(json!({}));
    }
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config file: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config file: {e}"))
    })
    .await
    .map_err(|e| format!("Config read task failed: {e}"))?
}

/// Write the config object back to the file, creating directories as needed.
/// Uses spawn_blocking to avoid blocking the Tokio runtime on slow filesystems.
async fn write_config(workspace: &std::path::Path, config: &Value) -> Result<(), String> {
    let path = config_path(workspace);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::write(&path, content).map_err(|e| format!("Failed to write config file: {e}"))
    })
    .await
    .map_err(|e| format!("Config write task failed: {e}"))?
}

const CONFIG_DESCRIPTION: &str = "View or modify configuration settings. \
Reads and writes `.autocoder/config.json` in the workspace. \
Supports actions: \"get\" (retrieve a key), \"set\" (update a key), \"list\" (show all settings).";

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &'static str {
        "Config"
    }

    fn description(&self) -> &'static str {
        CONFIG_DESCRIPTION
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "list"],
                    "description": "The config action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The configuration key (required for get and set)"
                },
                "value": {
                    "type": "string",
                    "description": "The value to set (required for set)"
                }
            }
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        match input["action"].as_str() {
            Some("get") | Some("list") => true,
            _ => false,
        }
    }

    fn scope(&self) -> ToolScope {
        // Reads/writes .autocoder/config.json which is main-process
        // session state — must not be forked into subtask workspaces.
        ToolScope::Session
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let action = match input["action"].as_str() {
            Some(a) if !a.is_empty() => a,
            _ => return ToolResult::err("Missing or empty 'action' parameter"),
        };

        match action {
            "list" => {
                let config = match read_config(ctx.workspace).await {
                    Ok(c) => c,
                    Err(e) => return ToolResult::err(e),
                };
                match serde_json::to_string_pretty(&config) {
                    Ok(s) => {
                        if config.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                            ToolResult::ok("No configuration settings found. Config file: .autocoder/config.json")
                        } else {
                            ToolResult::ok(format!("Current configuration:\n{s}"))
                        }
                    }
                    Err(e) => ToolResult::err(format!("Failed to format config: {e}")),
                }
            }
            "get" => {
                let key = match input["key"].as_str() {
                    Some(k) if !k.is_empty() => k,
                    _ => {
                        return ToolResult::err("Missing or empty 'key' parameter for 'get' action")
                    }
                };
                let config = match read_config(ctx.workspace).await {
                    Ok(c) => c,
                    Err(e) => return ToolResult::err(e),
                };
                match config.get(key) {
                    Some(value) => ToolResult::ok(format!("{key} = {value}")),
                    None => ToolResult::ok(format!("Key '{key}' is not set")),
                }
            }
            "set" => {
                let key = match input["key"].as_str() {
                    Some(k) if !k.is_empty() => k,
                    _ => {
                        return ToolResult::err("Missing or empty 'key' parameter for 'set' action")
                    }
                };
                let value = match input["value"].as_str() {
                    Some(v) => v,
                    None => return ToolResult::err("Missing 'value' parameter for 'set' action"),
                };
                let mut config = match read_config(ctx.workspace).await {
                    Ok(c) => c,
                    Err(e) => return ToolResult::err(e),
                };

                // Try to parse as JSON value (number, bool, null) or fall back to string
                let json_value: Value = serde_json::from_str(value)
                    .unwrap_or_else(|_| Value::String(value.to_string()));

                let obj: &mut serde_json::Map<String, Value> = match config.as_object_mut() {
                    Some(o) => o,
                    None => {
                        return ToolResult::err(
                            "Config file is corrupt (not a JSON object). \
                             Delete .autocoder/config.json and retry.",
                        )
                    }
                };
                let previous = obj.insert(key.to_string(), json_value.clone());

                if let Err(e) = write_config(ctx.workspace, &config).await {
                    return ToolResult::err(e);
                }

                match previous {
                    Some(prev) => ToolResult::ok(format!("Set {key} = {json_value} (was: {prev})")),
                    None => ToolResult::ok(format!("Set {key} = {json_value}")),
                }
            }
            other => ToolResult::err(format!(
                "Unknown action: '{other}'. Supported: get, set, list"
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
        let tool = ConfigTool;
        assert_eq!(tool.name(), "Config");
        assert!(tool.is_read_only(&json!({"action": "get"})));
        assert!(tool.is_read_only(&json!({"action": "list"})));
        assert!(!tool.is_read_only(&json!({"action": "set"})));
        assert!(tool.anthropic_builtin_type().is_none());
    }

    #[tokio::test]
    async fn test_list_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ConfigTool.execute(json!({"action": "list"}), &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("No configuration"));
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);

        // Set a value
        let result = ConfigTool
            .execute(
                json!({"action": "set", "key": "theme", "value": "dark"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Set theme"));

        // Get the value back
        let result = ConfigTool
            .execute(json!({"action": "get", "key": "theme"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("theme"));
        assert!(result.content.contains("dark"));
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ConfigTool
            .execute(json!({"action": "get", "key": "nonexistent"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("not set"));
    }

    #[tokio::test]
    async fn test_missing_action() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().canonicalize().unwrap();
        let ctx = make_ctx(&ws);
        let result = ConfigTool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("action"));
    }
}
