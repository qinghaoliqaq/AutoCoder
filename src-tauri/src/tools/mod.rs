/// Modular tool system — each tool is a self-contained module implementing the `Tool` trait.
///
/// ```text
/// tools/
///   mod.rs           ← Tool trait, ToolRegistry, helpers (this file)
///   bash.rs          ← BashTool
///   file_read.rs     ← FileReadTool
///   file_edit.rs     ← FileEditTool
///   file_write.rs    ← FileWriteTool
///   grep.rs          ← GrepTool
///   glob.rs          ← GlobTool
///   web_fetch.rs     ← WebFetchTool
///   web_search.rs    ← WebSearchTool
///   notebook_edit.rs ← NotebookEditTool
///   lsp.rs           ← LSPTool
///   agent.rs         ← AgentTool
///   ask_user.rs      ← AskUserQuestionTool
///   send_message.rs  ← SendMessageTool
///   sleep.rs         ← SleepTool
///   todo_write.rs    ← TodoWriteTool
///   skill.rs         ← SkillTool
///   tool_search.rs   ← ToolSearchTool
///   task_create.rs   ← TaskCreateTool
///   task_get.rs      ← TaskGetTool
///   task_list.rs     ← TaskListTool
///   task_update.rs   ← TaskUpdateTool
///   task_stop.rs     ← TaskStopTool
///   task_output.rs   ← TaskOutputTool
///   enter_plan.rs    ← EnterPlanModeTool
///   exit_plan.rs     ← ExitPlanModeTool
///   enter_worktree.rs← EnterWorktreeTool
///   exit_worktree.rs ← ExitWorktreeTool
///   team_create.rs   ← TeamCreateTool
///   team_delete.rs   ← TeamDeleteTool
///   mcp.rs           ← MCPTool
///   mcp_auth.rs      ← McpAuthTool
///   list_mcp.rs      ← ListMcpResourcesTool
///   read_mcp.rs      ← ReadMcpResourceTool
///   powershell.rs    ← PowerShellTool
///   repl.rs          ← REPLTool
///   config.rs        ← ConfigTool
///   brief.rs         ← BriefTool
///   remote_trigger.rs← RemoteTriggerTool
///   schedule_cron.rs ← ScheduleCronTool (create/delete/list)
///   synthetic_output.rs ← SyntheticOutputTool
///   path_utils.rs    ← Shared path resolution & security helpers
/// ```
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

// ── Tool submodules ──────────────────────────────────────────────────────────
pub mod bash;
pub mod file_read;
pub mod file_edit;
pub mod file_write;
pub mod grep;
pub mod glob_tool;
pub mod web_fetch;
pub mod web_search;
pub mod notebook_edit;
pub mod lsp;
pub mod agent;
pub mod ask_user;
pub mod send_message;
pub mod sleep;
pub mod todo_write;
pub mod skill;
pub mod tool_search;
pub mod task_create;
pub mod task_get;
pub mod task_list;
pub mod task_update;
pub mod task_stop;
pub mod task_output;
pub mod enter_plan;
pub mod exit_plan;
pub mod enter_worktree;
pub mod exit_worktree;
pub mod team_create;
pub mod team_delete;
pub mod mcp;
pub mod mcp_auth;
pub mod list_mcp;
pub mod read_mcp;
pub mod powershell;
pub mod repl;
pub mod config_tool;
pub mod brief;
pub mod remote_trigger;
pub mod schedule_cron;
pub mod synthetic_output;
pub mod path_utils;

use crate::tool_runner::providers::WireFormat;

// ── Constants ────────────────────────────────────────────────────────────────

const LARGE_RESULT_THRESHOLD: usize = 30_000;
const LARGE_RESULT_PREVIEW: usize = 2_000;
const MAX_RESULT_CHARS: usize = 50_000;

// ── Core types ───────────────────────────────────────────────────────────────

/// Result of a tool execution.
pub struct ToolResult {
    /// Text content returned to the model.
    pub content: String,
    /// Whether this result indicates an error.
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Execution context passed to every tool call.
pub struct ToolContext<'a> {
    /// Workspace root directory (canonicalized).
    pub workspace: &'a Path,
    /// Whether running in read-only mode (no writes allowed).
    pub read_only: bool,
    /// Cancellation token for long-running operations.
    pub token: &'a CancellationToken,
}

// ── Tool trait ───────────────────────────────────────────────────────────────

/// Every tool must implement this trait.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Machine name used in API requests (e.g. "Bash", "Read", "Edit").
    fn name(&self) -> &'static str;

    /// Human-readable description shown to the model.
    fn description(&self) -> &'static str;

    /// JSON Schema for the tool's input parameters.
    fn input_schema(&self) -> Value;

    /// Whether this specific invocation is read-only (safe for concurrent execution).
    fn is_read_only(&self, input: &Value) -> bool;

    /// Whether this invocation is destructive (delete, overwrite, etc.).
    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    /// Execute the tool with the given input and context.
    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult;

    /// If this tool maps to an Anthropic built-in type (e.g. bash_20250124),
    /// return the type string. Otherwise None = custom tool.
    fn anthropic_builtin_type(&self) -> Option<&'static str> {
        None
    }
}

// ── Tool Registry ────────────────────────────────────────────────────────────

/// Registry holding all available tools. Provides schema generation and dispatch.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    by_name: HashMap<String, usize>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a tool. Panics if a tool with the same name already exists.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        assert!(
            !self.by_name.contains_key(&name),
            "duplicate tool registration: {name}"
        );
        let idx = self.tools.len();
        self.by_name.insert(name, idx);
        self.tools.push(tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.by_name.get(name).map(|&idx| self.tools[idx].as_ref())
    }

    /// All registered tool names.
    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Generate tool definitions for the given wire format.
    pub fn definitions(&self, format: WireFormat) -> Vec<Value> {
        self.tools
            .iter()
            .map(|t| tool_to_definition(t.as_ref(), format))
            .collect()
    }

    /// Generate read-only tool definitions (only tools where is_read_only returns true
    /// for all inputs, or tools that have a read-only subset).
    pub fn read_only_definitions(&self, format: WireFormat) -> Vec<Value> {
        self.tools
            .iter()
            .filter_map(|t| tool_to_read_only_definition(t.as_ref(), format))
            .collect()
    }

    /// Check if a tool call is read-only.
    pub fn is_read_only(&self, name: &str, input: &Value) -> bool {
        self.get(name).map(|t| t.is_read_only(input)).unwrap_or(false)
    }

    /// Execute a tool by name.
    pub async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext<'_>,
    ) -> ToolResult {
        match self.get(name) {
            Some(tool) => {
                // Read-only enforcement: reject write tools in read-only mode
                if ctx.read_only && !tool.is_read_only(&input) {
                    return ToolResult::err(format!(
                        "{name}: blocked in read-only mode"
                    ));
                }
                tool.execute(input, ctx).await
            }
            None => ToolResult::err(format!("Unknown tool: {name}")),
        }
    }

    /// Summarize tool input for frontend tool-log display.
    pub fn summarize_input(&self, name: &str, input: &Value) -> String {
        match name {
            "Bash" => input["command"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(150)
                .collect(),
            "Edit" => {
                let path = input["file_path"].as_str().unwrap_or("");
                let old = input["old_string"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(50)
                    .collect::<String>();
                format!("{path}: {old}...")
            }
            "Read" => {
                let path = input["file_path"].as_str().unwrap_or("");
                format!("read {path}")
            }
            "Write" => {
                let path = input["file_path"].as_str().unwrap_or("");
                format!("write {path}")
            }
            "Grep" => {
                let pattern = input["pattern"].as_str().unwrap_or("");
                let path = input["path"].as_str().unwrap_or(".");
                format!("/{pattern}/ in {path}")
            }
            "Glob" => {
                let pattern = input["pattern"].as_str().unwrap_or("");
                format!("find {pattern}")
            }
            _ => serde_json::to_string(input)
                .unwrap_or_default()
                .chars()
                .take(150)
                .collect(),
        }
    }
}

// ── Schema generation helpers ────────────────────────────────────────────────

/// Convert a Tool to a wire-format tool definition.
fn tool_to_definition(tool: &dyn Tool, format: WireFormat) -> Value {
    match format {
        WireFormat::Anthropic => {
            if let Some(builtin) = tool.anthropic_builtin_type() {
                json!({ "type": builtin, "name": tool.name() })
            } else {
                json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "input_schema": tool.input_schema(),
                })
            }
        }
        WireFormat::OpenAI => {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.input_schema(),
                }
            })
        }
    }
}

/// Convert a Tool to a read-only wire-format definition, if applicable.
/// Returns None for tools that are inherently write-only.
fn tool_to_read_only_definition(tool: &dyn Tool, format: WireFormat) -> Option<Value> {
    let name = tool.name();
    match name {
        // Bash is excluded in read-only mode
        "Bash" | "PowerShell" => None,
        // Write tools excluded
        "Write" | "NotebookEdit" => None,
        // Edit becomes read-only (view only)
        "Edit" => Some(tool_to_definition(tool, format)),
        // Agent/task tools excluded in read-only
        "Agent" | "TaskCreate" | "TaskUpdate" | "TaskStop" | "TeamCreate" | "TeamDelete"
        | "EnterPlanMode" | "ExitPlanMode" | "EnterWorktree" | "ExitWorktree"
        | "ScheduleCron" | "RemoteTrigger" | "McpAuth" | "Brief" | "Config"
        | "SyntheticOutput" | "REPL" => None,
        // Everything else is allowed (search, read, info tools)
        _ => Some(tool_to_definition(tool, format)),
    }
}

// ── Default registry builder ─────────────────────────────────────────────────

/// Build the default tool registry with all 43 tools.
pub fn default_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();

    // Core file tools
    reg.register(Box::new(file_read::FileReadTool));
    reg.register(Box::new(file_edit::FileEditTool));
    reg.register(Box::new(file_write::FileWriteTool));

    // Search tools
    reg.register(Box::new(grep::GrepTool));
    reg.register(Box::new(glob_tool::GlobTool));

    // Shell execution
    reg.register(Box::new(bash::BashTool));
    reg.register(Box::new(powershell::PowerShellTool));
    reg.register(Box::new(repl::REPLTool));

    // Web tools
    reg.register(Box::new(web_fetch::WebFetchTool));
    reg.register(Box::new(web_search::WebSearchTool));

    // Editor / notebook
    reg.register(Box::new(notebook_edit::NotebookEditTool));

    // LSP
    reg.register(Box::new(lsp::LSPTool));

    // Agent + communication
    reg.register(Box::new(agent::AgentTool));
    reg.register(Box::new(ask_user::AskUserQuestionTool));
    reg.register(Box::new(send_message::SendMessageTool));
    reg.register(Box::new(sleep::SleepTool));
    reg.register(Box::new(todo_write::TodoWriteTool));
    reg.register(Box::new(skill::SkillTool));
    reg.register(Box::new(tool_search::ToolSearchTool));

    // Task management
    reg.register(Box::new(task_create::TaskCreateTool));
    reg.register(Box::new(task_get::TaskGetTool));
    reg.register(Box::new(task_list::TaskListTool));
    reg.register(Box::new(task_update::TaskUpdateTool));
    reg.register(Box::new(task_stop::TaskStopTool));
    reg.register(Box::new(task_output::TaskOutputTool));

    // Plan mode + worktree
    reg.register(Box::new(enter_plan::EnterPlanModeTool));
    reg.register(Box::new(exit_plan::ExitPlanModeTool));
    reg.register(Box::new(enter_worktree::EnterWorktreeTool));
    reg.register(Box::new(exit_worktree::ExitWorktreeTool));

    // Team
    reg.register(Box::new(team_create::TeamCreateTool));
    reg.register(Box::new(team_delete::TeamDeleteTool));

    // MCP
    reg.register(Box::new(mcp::MCPTool));
    reg.register(Box::new(mcp_auth::McpAuthTool));
    reg.register(Box::new(list_mcp::ListMcpResourcesTool));
    reg.register(Box::new(read_mcp::ReadMcpResourceTool));

    // Misc
    reg.register(Box::new(config_tool::ConfigTool));
    reg.register(Box::new(brief::BriefTool));
    reg.register(Box::new(remote_trigger::RemoteTriggerTool));
    reg.register(Box::new(schedule_cron::ScheduleCronTool));
    reg.register(Box::new(synthetic_output::SyntheticOutputTool));

    reg
}

// ── Partitioned execution ────────────────────────────────────────────────────

const MAX_TOOL_CONCURRENCY: usize = 10;

/// Execute tool calls with read-only batching (concurrent) and write (serial).
pub async fn run_partitioned(
    registry: &ToolRegistry,
    tool_calls: &[(String, String, Value)],
    workspace: &Path,
    token: &CancellationToken,
    read_only: bool,
) -> Result<Vec<Value>, String> {
    // Group consecutive read-only calls into batches
    let mut batches: Vec<(bool, Vec<usize>)> = Vec::new();
    for (i, (_id, name, input)) in tool_calls.iter().enumerate() {
        let is_ro = registry.is_read_only(name, input);
        if is_ro && batches.last().map(|b| b.0).unwrap_or(false) {
            batches.last_mut().unwrap().1.push(i);
        } else {
            batches.push((is_ro, vec![i]));
        }
    }

    let mut results: Vec<Value> = vec![Value::Null; tool_calls.len()];

    for (is_readonly, indices) in &batches {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }
        if *is_readonly && indices.len() > 1 {
            // Run read-only tools concurrently
            run_concurrent_batch(registry, tool_calls, indices, workspace, token, read_only, &mut results).await?;
        } else {
            // Run write tools serially
            for &idx in indices {
                if token.is_cancelled() {
                    return Err("cancelled".to_string());
                }
                let (id, name, input) = &tool_calls[idx];
                let ctx = ToolContext {
                    workspace,
                    read_only,
                    token,
                };
                let result = registry.execute(name, input.clone(), &ctx).await;
                results[idx] = build_tool_result(id, name, result);
            }
        }
    }

    Ok(results)
}

async fn run_concurrent_batch(
    registry: &ToolRegistry,
    tool_calls: &[(String, String, Value)],
    indices: &[usize],
    workspace: &Path,
    token: &CancellationToken,
    read_only: bool,
    results: &mut [Value],
) -> Result<(), String> {
    // We cannot easily move registry refs into spawned tasks, so we execute
    // concurrently via join_all on futures (no spawn needed for moderate concurrency).
    use futures::future::join_all;

    for chunk in indices.chunks(MAX_TOOL_CONCURRENCY) {
        let futs: Vec<_> = chunk
            .iter()
            .map(|&idx| {
                let (id, name, input) = &tool_calls[idx];
                let id = id.clone();
                let name_owned = name.clone();
                let input_owned = input.clone();
                async move {
                    let ctx = ToolContext {
                        workspace,
                        read_only,
                        token,
                    };
                    let result = registry.execute(&name_owned, input_owned, &ctx).await;
                    (idx, id, name_owned, result)
                }
            })
            .collect();

        let batch_results = join_all(futs).await;
        for (idx, id, name, result) in batch_results {
            results[idx] = build_tool_result(&id, &name, result);
        }
    }
    Ok(())
}

fn build_tool_result(id: &str, tool_name: &str, result: ToolResult) -> Value {
    let content = maybe_persist_large_result(&result.content, tool_name);
    let mut obj = json!({
        "type": "tool_result",
        "tool_use_id": id,
        "content": content,
    });
    if result.is_error {
        obj["is_error"] = json!(true);
    }
    obj
}

// ── Large result persistence ─────────────────────────────────────────────────

fn result_cache_dir() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("ai-dev-hub").join("tool-results")
}

fn maybe_persist_large_result(result: &str, tool_name: &str) -> String {
    if result.len() <= LARGE_RESULT_THRESHOLD {
        return result.to_string();
    }
    let cache_dir = result_cache_dir();
    if std::fs::create_dir_all(&cache_dir).is_err() {
        return truncate_result(result);
    }
    let ts = chrono::Utc::now().timestamp_millis();
    let path = cache_dir.join(format!("{tool_name}_{ts}.txt"));
    if std::fs::write(&path, result).is_ok() {
        let preview_end = result
            .char_indices()
            .nth(LARGE_RESULT_PREVIEW)
            .map(|(i, _)| i)
            .unwrap_or(result.len());
        format!(
            "{}\n\n... [result too large: {} chars, saved to {}]",
            &result[..preview_end],
            result.len(),
            path.display(),
        )
    } else {
        truncate_result(result)
    }
}

fn truncate_result(result: &str) -> String {
    if result.len() > MAX_RESULT_CHARS {
        format!(
            "{}...\n[output truncated at {} chars]",
            &result[..MAX_RESULT_CHARS],
            MAX_RESULT_CHARS
        )
    } else {
        result.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(sleep::SleepTool));
        assert!(reg.get("Sleep").is_some());
        assert!(reg.get("nonexistent").is_none());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn tool_result_constructors() {
        let ok = ToolResult::ok("success");
        assert!(!ok.is_error);
        assert_eq!(ok.content, "success");

        let err = ToolResult::err("failed");
        assert!(err.is_error);
        assert_eq!(err.content, "failed");
    }

    #[test]
    fn default_registry_has_all_tools() {
        let reg = default_registry();
        assert!(reg.len() >= 40, "expected 40+ tools, got {}", reg.len());
    }
}
