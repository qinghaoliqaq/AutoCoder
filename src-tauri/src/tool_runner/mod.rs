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
use crate::tools::ask_user_question::registry::TauriUserQuestionAsker;
use crate::tools::sub_agent_runner::{SubAgentRequest, SubAgentRunner};
use crate::tools::{self, OrchestrationCtx, ToolRegistry};
use async_trait::async_trait;
use providers::{ProviderConfig, WireFormat};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

// ── Production sub-agent runner ──────────────────────────────────────────────

/// Wires `SubAgentRunner` to the existing `run_subtask` /
/// `run_read_only_subtask` entry points. Stateless — held as a `static`
/// reference inside `OrchestrationCtx`.
pub struct ProductionSubAgentRunner;

#[async_trait]
impl SubAgentRunner for ProductionSubAgentRunner {
    async fn run(&self, req: SubAgentRequest<'_>) -> Result<String, String> {
        let cwd = req.workspace.to_string_lossy().into_owned();
        if req.read_only {
            run_read_only_subtask(
                req.config,
                req.system_prompt,
                req.user_prompt,
                Some(&cwd),
                req.window_label,
                req.app_handle,
                req.token,
                req.subtask_id,
            )
            .await
        } else {
            run_subtask(
                req.config,
                req.system_prompt,
                req.user_prompt,
                Some(&cwd),
                req.window_label,
                req.app_handle,
                req.token,
                req.subtask_id,
            )
            .await
        }
    }
}

static PRODUCTION_SUB_AGENT_RUNNER: ProductionSubAgentRunner = ProductionSubAgentRunner;
static PRODUCTION_USER_QUESTION_ASKER: TauriUserQuestionAsker = TauriUserQuestionAsker;

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

    // `in_subtask` is true when this runner was dispatched with a
    // subtask_id — see `tools::ToolScope` for what gets filtered.
    let in_subtask = subtask_id.is_some();
    let registry = tools::registry();
    let tool_defs = registry.definitions(provider.wire, read_only, in_subtask);

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

    // Build orchestration context once per top-level run and thread it
    // through both wire formats. Sub-agent / user-question tools reach into
    // this context to drive nested loops and UI prompts.
    let agent_id = subtask_id.unwrap_or("main");
    let orch = OrchestrationCtx {
        config,
        app_handle,
        window_label,
        agent_id,
        sub_agent_runner: &PRODUCTION_SUB_AGENT_RUNNER,
        user_question_asker: &PRODUCTION_USER_QUESTION_ASKER,
    };

    let loop_result = match provider.wire {
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
                registry,
                window_label,
                app_handle,
                token,
                read_only,
                subtask_id,
                Some(&orch),
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
                registry,
                window_label,
                app_handle,
                token,
                read_only,
                subtask_id,
                Some(&orch),
            )
            .await
        }
    };

    // Fire Stop hooks on top-level runs only. Subtasks emit their own
    // `PostToolUse` after the parent's `StartSubAgent` returns, which is
    // the right granularity — nesting Stop on every subtask would flood
    // notification-style hooks.
    //
    // Cancellation: even if the user hit Ctrl-C, run Stop hooks against
    // a fresh token so post-mortem cleanup (e.g. `notify-send`,
    // `git status`) still fires. Per-hook timeouts still apply.
    if subtask_id.is_none() && !config.hooks.stop.is_empty() {
        let stop_token = CancellationToken::new();
        let _ = crate::hooks::stop(&config.hooks, &workspace, &stop_token, agent_id).await;
    }

    loop_result
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

/// Map a runner mode to the agent label surfaced in UI events.
///
/// The read-only path is driven by the *second* provider slot (see
/// `run_inner` — it calls `ProviderConfig::from_app_config_second`), which the
/// product treats as the "Codex" reviewer role in plan mode.  The writer path
/// is the primary provider, labeled "claude" in the UI.  Without this mapping
/// every event gets stamped "claude" and the Codex reviewer appears to vanish.
fn agent_label(read_only: bool) -> &'static str {
    if read_only {
        "codex"
    } else {
        "claude"
    }
}

fn emit_chunk(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    text: &str,
    is_first_chunk: &mut bool,
    subtask_id: Option<&str>,
    read_only: bool,
) {
    let reset = *is_first_chunk;
    *is_first_chunk = false;
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "skill-chunk",
        SkillChunk {
            agent: agent_label(read_only).to_string(),
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
    read_only: bool,
) {
    let ts = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let summary = registry.summarize_input(name, input);
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "tool-log",
        ToolLog {
            agent: agent_label(read_only).to_string(),
            tool: name.to_string(),
            input: summary,
            timestamp: ts,
        },
    );
}
