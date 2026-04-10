/// Modular tool system — each tool is a self-contained module implementing the `Tool` trait.
///
/// ```text
/// tools/
///   mod.rs            ← Tool trait, ToolRegistry, helpers (this file)
///   path_utils.rs     ← Shared path resolution & security helpers
///   bash/             ← BashTool (shell execution)
///   file_read/        ← FileReadTool
///   file_edit/        ← FileEditTool
///   file_write/       ← FileWriteTool
///   grep/             ← GrepTool (ripgrep)
///   glob_tool/        ← GlobTool (file pattern matching)
///   web_fetch/        ← WebFetchTool
///   notebook_edit/    ← NotebookEditTool
///   sleep/            ← SleepTool
///   todo_write/       ← TodoWriteTool (task tracking)
///   skill/            ← SkillTool (skill/slash command invocation)
///   enter_worktree/   ← EnterWorktreeTool (git worktree)
///   exit_worktree/    ← ExitWorktreeTool (git worktree)
///   mcp/              ← MCPTool (MCP server tools)
///   mcp_auth/         ← McpAuthTool (MCP authentication)
///   list_mcp/         ← ListMcpResourcesTool
///   read_mcp/         ← ReadMcpResourceTool
///   powershell/       ← PowerShellTool (Windows)
///   repl/             ← REPLTool (Python/Node/Ruby)
///   config_tool/      ← ConfigTool (view/modify config)
///   schedule_cron/    ← ScheduleCronTool (scheduled tasks)
/// ```
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

// ── Tool submodules ──────────────────────────────────────────────────────────
pub mod bash;
pub mod config_tool;
pub mod enter_worktree;
pub mod exit_worktree;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob_tool;
pub mod grep;
pub mod list_mcp;
pub mod mcp;
pub mod mcp_auth;
pub mod notebook_edit;
pub mod path_utils;
pub mod powershell;
pub mod read_mcp;
pub mod repl;
pub mod schedule_cron;
pub mod skill;
pub mod sleep;
pub mod todo_write;
pub mod web_fetch;

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

// ── Tool scope ───────────────────────────────────────────────────────────────

/// Whether a tool is safe to run inside a subtask's isolated workspace copy
/// or must only run against main-process session state.
///
/// The orchestrator forks the project workspace into isolated copies when
/// running parallel subtasks, then 3-way-merges the results back on
/// completion.  Tools that write to `.autocoder/` book-keeping files
/// (`todos.json`, `config.json`, `cron.json`) are conceptually **session**
/// state — they coordinate the orchestrator itself, not the user's project.
/// If a forked subtask writes to them, two parallel subtasks end up with
/// mutually incompatible versions and the merge engine reports spurious
/// conflicts like:
///
/// ```text
/// Subtask F15 hit merge conflicts after 5 attempts:
/// Merge conflict on files already changed in the main workspace: .autocoder/todos.json
/// ```
///
/// Session-scoped tools are filtered out of the tool schema exposed to
/// subtasks AND rejected by `ToolRegistry::execute` when called from a
/// subtask context — defense in depth at both the schema and dispatch
/// layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolScope {
    /// Operates on project files (Read, Write, Edit, Bash, Grep, …).
    /// Safe to run in an isolated subtask workspace.  This is the default.
    Workspace,
    /// Manages main-process session state (TodoWrite, Config, ScheduleCron).
    /// Must only run against the primary workspace, never a subtask fork.
    Session,
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

    /// Whether this tool is workspace-scoped (default) or session-scoped.
    /// Session-scoped tools are filtered out of subtask tool schemas and
    /// cannot be dispatched from within a subtask context.  See
    /// [`ToolScope`] for the full rationale.
    fn scope(&self) -> ToolScope {
        ToolScope::Workspace
    }

    /// Whether this specific invocation is destructive (potentially harmful).
    #[allow(dead_code)]
    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    /// Execute the tool with the given input and context.
    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult;

    /// Detailed usage prompt for the system prompt. Tells the model when to use
    /// this tool, best practices, and what to avoid. This is injected into the
    /// system prompt so the model understands how to use each tool properly.
    /// Returns None if no special prompt is needed (description is sufficient).
    fn prompt(&self) -> Option<&'static str> {
        None
    }

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

    /// Number of registered tools.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.by_name.get(name).map(|&idx| self.tools[idx].as_ref())
    }

    /// Build a combined tool-usage instruction section for injection into the
    /// system prompt. Each tool's `prompt()` is included under a heading with
    /// the tool name so the model knows exactly when/how to use each tool.
    pub fn tool_prompts(&self) -> String {
        let mut sections = Vec::new();
        for tool in &self.tools {
            if let Some(prompt) = tool.prompt() {
                sections.push(format!("## {}\n\n{}", tool.name(), prompt));
            }
        }
        if sections.is_empty() {
            return String::new();
        }
        format!(
            "# Tool Usage Instructions\n\n{}\n",
            sections.join("\n\n---\n\n")
        )
    }

    /// Generate tool definitions for the given wire format.
    ///
    /// Filters apply additively:
    ///
    /// * `read_only` — when `true`, drops tools that cannot run in read-only
    ///   mode (Bash, Write, Edit, TodoWrite, …).  Used by the Codex reviewer
    ///   path in plan mode.
    /// * `in_subtask` — when `true`, drops tools marked [`ToolScope::Session`]
    ///   (TodoWrite, Config, ScheduleCron).  Used when the runner is
    ///   dispatched into a subtask's isolated workspace copy — session
    ///   tools would race with parallel subtasks on `.autocoder/` files
    ///   and corrupt the merge-back step.
    pub fn definitions(&self, format: WireFormat, read_only: bool, in_subtask: bool) -> Vec<Value> {
        self.tools
            .iter()
            .filter_map(|t| {
                if in_subtask && matches!(t.scope(), ToolScope::Session) {
                    return None;
                }
                if read_only {
                    tool_to_read_only_definition(t.as_ref(), format)
                } else {
                    Some(tool_to_definition(t.as_ref(), format))
                }
            })
            .collect()
    }

    /// Check if a tool call is read-only.
    pub fn is_read_only(&self, name: &str, input: &Value) -> bool {
        self.get(name)
            .map(|t| t.is_read_only(input))
            .unwrap_or(false)
    }

    /// Execute a tool by name.
    ///
    /// `in_subtask` must be `true` when dispatching inside a subtask's
    /// isolated workspace copy; it causes session-scoped tools to be
    /// rejected even if the model manages to call them despite the schema
    /// filter in [`Self::definitions`].
    pub async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext<'_>,
        in_subtask: bool,
    ) -> ToolResult {
        match self.get(name) {
            Some(tool) => {
                // Scope enforcement (defense in depth — the schema filter
                // in `definitions` already hides session tools from
                // subtask models, but reject here too in case the model
                // fabricates a call to a tool it wasn't shown).
                if in_subtask && matches!(tool.scope(), ToolScope::Session) {
                    return ToolResult::err(format!(
                        "{name}: session-scoped tool cannot run inside a subtask \
                         (it would race with parallel subtasks on main-process \
                         book-keeping state — use it from the main orchestrator)"
                    ));
                }
                // Read-only enforcement: reject write tools in read-only mode
                if ctx.read_only && !tool.is_read_only(&input) {
                    return ToolResult::err(format!("{name}: blocked in read-only mode"));
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
        // Shell execution excluded in read-only mode
        "Bash" | "PowerShell" | "REPL" => None,
        // Write/edit tools excluded
        "Write" | "Edit" | "NotebookEdit" | "TodoWrite" => None,
        // Mutating tools excluded in read-only
        "EnterWorktree" | "ExitWorktree" | "ScheduleCron" | "McpAuth" | "Config" => None,
        // Everything else is allowed (search, read, info tools)
        _ => Some(tool_to_definition(tool, format)),
    }
}

// ── Default registry builder ─────────────────────────────────────────────────

/// Build the default tool registry with all tools.
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

    // Web
    reg.register(Box::new(web_fetch::WebFetchTool));

    // Editor / notebook
    reg.register(Box::new(notebook_edit::NotebookEditTool));

    // Session management
    reg.register(Box::new(sleep::SleepTool));
    reg.register(Box::new(todo_write::TodoWriteTool));
    reg.register(Box::new(skill::SkillTool));
    reg.register(Box::new(config_tool::ConfigTool));
    reg.register(Box::new(schedule_cron::ScheduleCronTool));

    // Git worktree
    reg.register(Box::new(enter_worktree::EnterWorktreeTool));
    reg.register(Box::new(exit_worktree::ExitWorktreeTool));

    // MCP / Skills integration
    reg.register(Box::new(mcp::MCPTool));
    reg.register(Box::new(mcp_auth::McpAuthTool));
    reg.register(Box::new(list_mcp::ListMcpResourcesTool));
    reg.register(Box::new(read_mcp::ReadMcpResourceTool));

    reg
}

// ── Partitioned execution ────────────────────────────────────────────────────

const MAX_TOOL_CONCURRENCY: usize = 10;

/// Execute tool calls with read-only batching (concurrent) and write (serial).
///
/// `in_subtask` must be `true` when this runner is dispatching inside a
/// subtask's isolated workspace copy, so that session-scoped tools are
/// rejected at dispatch time in addition to being filtered from the
/// schema.  See [`ToolScope`] for details.
pub async fn run_partitioned(
    registry: &ToolRegistry,
    tool_calls: &[(String, String, Value)],
    workspace: &Path,
    token: &CancellationToken,
    read_only: bool,
    in_subtask: bool,
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
            run_concurrent_batch(
                registry,
                tool_calls,
                indices,
                workspace,
                token,
                read_only,
                in_subtask,
                &mut results,
            )
            .await?;
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
                let result = registry
                    .execute(name, input.clone(), &ctx, in_subtask)
                    .await;
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
    in_subtask: bool,
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
                    let result = registry
                        .execute(&name_owned, input_owned, &ctx, in_subtask)
                        .await;
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
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = chrono::Utc::now().timestamp_millis();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = cache_dir.join(format!("{tool_name}_{ts}_{seq}.txt"));
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
        // Find a valid char boundary at or before MAX_RESULT_CHARS to avoid
        // panicking on multi-byte UTF-8 sequences.
        let end = (0..=MAX_RESULT_CHARS)
            .rev()
            .find(|&i| result.is_char_boundary(i))
            .unwrap_or(0);
        format!(
            "{}...\n[output truncated at {} chars]",
            &result[..end],
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
        assert!(reg.len() >= 21, "expected 21+ tools, got {}", reg.len());
    }

    #[test]
    fn tool_trait_default_scope_is_workspace() {
        // A tool that doesn't override scope() should default to Workspace,
        // so this is the safe default for new tools.
        assert_eq!(sleep::SleepTool.scope(), ToolScope::Workspace);
    }

    #[test]
    fn session_tools_are_marked_session_scope() {
        // The three tools that write to .autocoder/ must all be Session-scoped
        // so they get filtered out of subtask tool schemas.
        assert_eq!(todo_write::TodoWriteTool.scope(), ToolScope::Session);
        assert_eq!(config_tool::ConfigTool.scope(), ToolScope::Session);
        assert_eq!(schedule_cron::ScheduleCronTool.scope(), ToolScope::Session);
    }

    #[test]
    fn definitions_hide_session_tools_from_subtasks() {
        let reg = default_registry();
        let subtask_defs = reg.definitions(WireFormat::Anthropic, false, true);

        // Collect names from the generated schema
        let names: Vec<&str> = subtask_defs
            .iter()
            .filter_map(|d| d["name"].as_str())
            .collect();

        assert!(
            !names.contains(&"TodoWrite"),
            "TodoWrite must be hidden from subtask schema"
        );
        assert!(
            !names.contains(&"Config"),
            "Config must be hidden from subtask schema"
        );
        assert!(
            !names.contains(&"ScheduleCron"),
            "ScheduleCron must be hidden from subtask schema"
        );

        // Workspace tools are still there
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Bash"));
    }

    #[test]
    fn definitions_expose_session_tools_outside_subtasks() {
        let reg = default_registry();
        let main_defs = reg.definitions(WireFormat::Anthropic, false, false);
        let names: Vec<&str> = main_defs
            .iter()
            .filter_map(|d| d["name"].as_str())
            .collect();

        // Main orchestrator sees session tools — that's the entire point,
        // the main process is the owner of .autocoder/ state.
        assert!(names.contains(&"TodoWrite"));
        assert!(names.contains(&"Config"));
        assert!(names.contains(&"ScheduleCron"));
    }

    #[tokio::test]
    async fn execute_rejects_session_tool_in_subtask_context() {
        let reg = default_registry();
        let token = CancellationToken::new();
        let tmp = std::env::temp_dir();
        let ctx = ToolContext {
            workspace: &tmp,
            read_only: false,
            token: &token,
        };

        // Dispatching TodoWrite with in_subtask=true must fail fast, even
        // if the model manages to call it (e.g. hallucinated the name).
        let result = reg
            .execute(
                "TodoWrite",
                json!({ "todos": [] }),
                &ctx,
                true, // in_subtask
            )
            .await;

        assert!(result.is_error, "expected an error result");
        assert!(
            result.content.contains("session-scoped"),
            "expected 'session-scoped' in error message, got: {}",
            result.content
        );
    }
}
