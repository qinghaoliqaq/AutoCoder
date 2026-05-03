//! `SubAgentRunner` — abstraction layer between sub-agent-spawning tools
//! (e.g. `StartSubAgentTool`) and the actual agent loop in `tool_runner`.
//!
//! Two concrete impls live in different places:
//!   * `crate::tool_runner::ProductionSubAgentRunner` — wires to the real
//!     `tool_runner::run_subtask` / `run_read_only_subtask` functions.
//!   * test-only fakes — used by unit tests for `StartSubAgentTool` so we
//!     don't need to spin up an LLM provider to verify dispatch logic.
//!
//! The trait lives in the `tools` crate so tools can depend on it without
//! pulling `tool_runner` (which would create a circular module dependency).

use crate::config::AppConfig;
use async_trait::async_trait;
use std::path::Path;
use tokio_util::sync::CancellationToken;

/// All inputs needed to launch a single sub-agent. Built per call by the
/// orchestrator and passed through `SubAgentRunner::run`.
pub struct SubAgentRequest<'a> {
    pub config: &'a AppConfig,
    pub app_handle: &'a tauri::AppHandle,
    pub workspace: &'a Path,
    pub window_label: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    /// Identifier surfaced in `skill-chunk` / `tool-log` / `token-usage`
    /// events so the UI can group sub-agent output under its parent.
    pub subtask_id: &'a str,
    /// True → read-only loop (no Bash/Edit/Write). Reuses the second-slot
    /// provider config, matching the Codex reviewer convention.
    pub read_only: bool,
    pub token: CancellationToken,
}

#[async_trait]
pub trait SubAgentRunner: Send + Sync {
    async fn run(&self, request: SubAgentRequest<'_>) -> Result<String, String>;
}
