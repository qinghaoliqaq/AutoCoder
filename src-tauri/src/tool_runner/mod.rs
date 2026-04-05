/// Tool-use agent loop — modular architecture.
///
/// ```text
/// tool_runner/
///   mod.rs        ← public API (this file)
///   providers.rs  ← provider registry (Anthropic, OpenAI, Zhipu, MiniMax, ...)
///   tools.rs      ← tool schemas + read-only detection
///   execute.rs    ← local tool execution + partitioned orchestration
///   anthropic.rs  ← Anthropic Messages API loop
///   openai.rs     ← OpenAI-compatible Chat Completions loop
/// ```
///
/// All tool execution is 100% local Rust. Only the API wire format differs
/// between providers. Adding a new provider = one entry in providers.rs.

pub mod providers;
mod anthropic;
mod execute;
mod openai;
mod tools;

use crate::config::AppConfig;
use crate::skills::{SkillChunk, ToolLog};
use providers::{ProviderConfig, WireFormat};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

const MAX_LOOP_ITERATIONS: usize = 40;
const MAX_RESPONSE_TOKENS: u32 = 16384;

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
    let provider = ProviderConfig::from_app_config(config);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let workspace = cwd
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let tool_defs = tools::definitions(provider.wire);

    match provider.wire {
        WireFormat::Anthropic => {
            anthropic::run_loop(
                &client, &provider.base_url, &provider.api_key, &provider.model,
                system_prompt, user_prompt, &tool_defs, &workspace,
                window_label, app_handle, token,
            )
            .await
        }
        WireFormat::OpenAI => {
            openai::run_loop(
                &client, &provider.base_url, &provider.api_key, &provider.model,
                system_prompt, user_prompt, &tool_defs, &workspace,
                window_label, app_handle, token,
            )
            .await
        }
    }
}

// ── Emit helpers (shared by anthropic.rs and openai.rs) ─────────────────────

fn emit_chunk(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    text: &str,
    is_first_chunk: &mut bool,
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
        },
    );
}

fn emit_tool_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    name: &str,
    input: &Value,
) {
    let ts = chrono::Utc::now().timestamp_millis() as u64;
    let summary = tools::summarize_input(name, input);
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
