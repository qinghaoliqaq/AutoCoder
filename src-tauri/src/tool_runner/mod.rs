mod anthropic;
/// Tool-use agent loop — modular architecture.
///
/// ```text
/// tool_runner/
///   mod.rs           ← public API (this file)
///   providers.rs     ← provider registry (Anthropic, OpenAI, Zhipu, MiniMax, ...)
///   anthropic.rs     ← Anthropic Messages API loop (SSE streaming)
///   openai.rs        ← OpenAI-compatible Chat Completions loop (SSE streaming)
///   system_prompt.rs ← comprehensive base system prompt (adapted from Claw)
/// ```
///
/// Tool definitions and execution are now provided by the `crate::tools` module.
/// The tool_runner only handles the API loop and event emission.
pub(crate) mod errors;
mod openai;
pub mod providers;
mod system_prompt;

use crate::config::AppConfig;
use crate::skills::{SkillChunk, TokenUsage, ToolLog};
use crate::tools::{self, ToolRegistry};
use providers::{ProviderConfig, WireFormat};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

const MAX_LOOP_ITERATIONS: usize = 40;
const MAX_RESPONSE_TOKENS: u32 = 16384;

/// Default context budget for the agent tool-use loop (in tokens).
/// Models vary (Claude: 200K, GPT-4o: 128K, DeepSeek: 64K), so we use
/// a conservative value that works for most modern models.
const CONTEXT_BUDGET_TOKENS: u64 = 100_000;

/// When input_tokens exceeds this fraction of the budget, prune old rounds.
const PRUNE_THRESHOLD: f64 = 0.75;

// ── Public API ──────────────────────────────────────────────────────────────

/// Run a tool-use agent loop. Auto-detects provider and wire format from config.
pub async fn run(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_inner(
        config,
        system_prompt,
        user_prompt,
        cwd,
        window_label,
        app_handle,
        token,
        false,
        None,
    )
    .await
}

/// Run a read-only tool-use agent loop (no bash, editor view-only).
/// Used for diagnostic/analysis phases that must not mutate the workspace.
/// Defense-in-depth: schema excludes write tools AND dispatch rejects them at runtime.
pub async fn run_read_only(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    run_inner(
        config,
        system_prompt,
        user_prompt,
        cwd,
        window_label,
        app_handle,
        token,
        true,
        None,
    )
    .await
}

/// Run a tool-use agent loop with subtask tagging.
/// Emitted `skill-chunk` events carry the given `subtask_id`.
pub async fn run_subtask(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    subtask_id: &str,
) -> Result<String, String> {
    run_inner(
        config,
        system_prompt,
        user_prompt,
        cwd,
        window_label,
        app_handle,
        token,
        false,
        Some(subtask_id),
    )
    .await
}

/// Run a read-only tool-use agent loop with subtask tagging.
pub async fn run_read_only_subtask(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    subtask_id: &str,
) -> Result<String, String> {
    run_inner(
        config,
        system_prompt,
        user_prompt,
        cwd,
        window_label,
        app_handle,
        token,
        true,
        Some(subtask_id),
    )
    .await
}

async fn run_inner(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    read_only: bool,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    let provider = if read_only {
        ProviderConfig::from_app_config_second(config)
    } else {
        ProviderConfig::from_app_config(config)
    };

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let workspace = canonicalize_workspace(cwd)?;

    // Build tool registry and generate definitions for the wire format
    let registry = tools::default_registry();
    let tool_defs = if read_only {
        registry.read_only_definitions(provider.wire)
    } else {
        registry.definitions(provider.wire)
    };

    // Build comprehensive system prompt:
    //   1. Base system prompt (adapted from Claw — behavior, safety, tool usage)
    //   2. Skill-specific system prompt (provided by the caller)
    //   3. Per-tool usage instructions (from ToolRegistry::tool_prompts())
    let base_prompt =
        system_prompt::build_base_prompt(&provider.model, &workspace.to_string_lossy());
    let tool_instructions = registry.tool_prompts();
    let full_system_prompt = {
        let mut parts = vec![base_prompt];
        if !system_prompt.is_empty() {
            parts.push(system_prompt.to_string());
        }
        if !tool_instructions.is_empty() {
            parts.push(tool_instructions);
        }
        parts.join("\n\n")
    };

    match provider.wire {
        WireFormat::Anthropic => {
            anthropic::run_loop(
                &client,
                &provider.base_url,
                &provider.api_key,
                &provider.model,
                &full_system_prompt,
                user_prompt,
                &tool_defs,
                &workspace,
                &registry,
                window_label,
                app_handle,
                token,
                read_only,
                subtask_id,
            )
            .await
        }
        WireFormat::OpenAI => {
            openai::run_loop(
                &client,
                &provider.base_url,
                &provider.api_key,
                &provider.model,
                &full_system_prompt,
                user_prompt,
                &tool_defs,
                &workspace,
                &registry,
                window_label,
                app_handle,
                token,
                read_only,
                subtask_id,
            )
            .await
        }
    }
}

// ── Workspace canonicalization ──────────────────────────────────────────────

/// Resolve and canonicalize the workspace path at the entry point.
/// This ensures all downstream path-containment checks compare against
/// a canonical base, preventing traversal via symlinks or `..` components.
fn canonicalize_workspace(cwd: Option<&str>) -> Result<PathBuf, String> {
    let raw = cwd
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    raw.canonicalize().map_err(|e| {
        format!(
            "workspace path error: cannot canonicalize '{}': {e}",
            raw.display()
        )
    })
}

// ── Emit helpers (shared by anthropic.rs and openai.rs) ─────────────────────

fn emit_chunk(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    text: &str,
    is_first_chunk: &mut bool,
    subtask_id: Option<&str>,
) {
    let reset = *is_first_chunk;
    *is_first_chunk = false;
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "skill-chunk",
        SkillChunk {
            agent: "claude".to_string(),
            text: text.to_string(),
            reset,
            subtask_id: subtask_id.map(ToString::to_string),
        },
    );
}

fn emit_token_usage(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    input_tokens: u64,
    output_tokens: u64,
    subtask_id: Option<&str>,
) {
    if input_tokens == 0 && output_tokens == 0 {
        return;
    }
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "token-usage",
        TokenUsage {
            input_tokens,
            output_tokens,
            subtask_id: subtask_id.map(ToString::to_string),
        },
    );
}

fn emit_tool_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    name: &str,
    input: &Value,
    registry: &ToolRegistry,
) {
    let ts = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let summary = registry.summarize_input(name, input);
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "tool-log",
        ToolLog {
            agent: "claude".to_string(),
            tool: name.to_string(),
            input: summary,
            timestamp: ts,
        },
    );
}
