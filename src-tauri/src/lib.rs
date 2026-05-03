pub(crate) mod bundled_skills;
mod config;
mod director;
mod history;
pub(crate) mod hooks;
pub(crate) mod memory;
mod prompts;
mod skills;
pub(crate) mod tool_runner;
pub(crate) mod tools;
mod workspace;

use config::{AppConfig, ConfigDraft, ConfigStatus, ExecutionAccessMode};
use director::chat_with_director;
use prompts::Prompts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tool_runner::errors::SkillError;
use tool_runner::providers::{self, ResolvedProviderInfo};
use tracing::{info, warn};

pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub prompts: Arc<Prompts>,
    pub histories: Mutex<HashMap<String, Vec<Value>>>,
    /// Per-window cancellation tokens for skill runs.
    pub cancel_tokens: Mutex<HashMap<String, CancellationToken>>,
    pub test_workspaces: Mutex<HashMap<String, String>>,
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub api_configured: bool,
    pub api_provider: String,
    pub api_model: String,
}

#[tauri::command]
fn detect_tools(state: tauri::State<'_, AppState>) -> SystemStatus {
    let config = state.config.read().unwrap_or_else(|e| e.into_inner());
    let api_configured = config.agent.is_configured() || config.is_configured();
    let api_provider = if config.agent.is_configured() {
        config.agent.provider.clone()
    } else {
        config.director.effective_provider()
    };
    let api_model = if config.agent.is_configured() {
        config.agent.model.clone()
    } else {
        config.director.model.clone()
    };
    SystemStatus {
        api_configured,
        api_provider,
        api_model,
    }
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> ConfigStatus {
    state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .status()
}

#[tauri::command]
fn get_config_form(_state: tauri::State<'_, AppState>) -> ConfigDraft {
    config::AppConfig::load_persisted()
        .unwrap_or_default()
        .draft()
}

#[tauri::command]
fn sanitize_blackboard_state(path: String) -> Result<(), String> {
    skills::sanitize_blackboard_state(&path)
}

#[tauri::command]
fn save_config(
    config: ConfigDraft,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigStatus, String> {
    let effective = AppConfig::persist_draft(config)?;
    let status = effective.status();
    *state.config.write().unwrap_or_else(|e| e.into_inner()) = effective;
    Ok(status)
}

#[tauri::command]
fn set_execution_access_mode(
    mode: ExecutionAccessMode,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigStatus, String> {
    let mut draft = config::AppConfig::load_persisted()
        .unwrap_or_default()
        .draft();
    draft.execution_access_mode = mode;
    let effective = AppConfig::persist_draft(draft)?;
    let status = effective.status();
    *state.config.write().unwrap_or_else(|e| e.into_inner()) = effective;
    Ok(status)
}

#[tauri::command]
async fn director_chat(
    input: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let config = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let window_label = window.label().to_string();
    info!(window = %window_label, "director chat started");
    let token = CancellationToken::new();
    {
        let mut tokens = state
            .cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        tokens.insert(window_label.clone(), token.clone());
    }
    let result = chat_with_director(
        &config,
        &state.prompts,
        &input,
        &state.histories,
        &window_label,
        &app_handle,
        token,
    )
    .await;
    state
        .cancel_tokens
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&window_label);
    match &result {
        Err(e) if e == "cancelled" => info!("director chat cancelled"),
        Err(e) => warn!(error = %e, "director chat failed"),
        Ok(()) => info!("director chat completed"),
    }
    // Treat cancellation as a clean stop rather than an error surfaced to the UI.
    match result {
        Err(e) if e == "cancelled" => Ok(()),
        other => other,
    }
}

#[tauri::command]
fn clear_history(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>) {
    let window_label = window.label();
    director::clear_history(&state.histories, window_label);
}

#[tauri::command]
fn get_director_history(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Vec<Value> {
    let window_label = window.label();
    director::get_history(&state.histories, window_label)
}

#[tauri::command]
fn restore_director_history(
    history: Vec<Value>,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) {
    let window_label = window.label();
    director::set_history(&state.histories, window_label, history);
}

#[tauri::command]
async fn run_skill(
    mode: String,
    task: String,
    workspace: Option<String>,
    phase: Option<String>,
    context: Option<String>,
    issue: Option<String>,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), SkillError> {
    let config = state
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let window_label = window.label().to_string();
    info!(mode = %mode, phase = ?phase, window = %window_label, "skill started");
    // Create a fresh cancellation token for this run, replacing any previous one.
    let token = CancellationToken::new();
    {
        let mut tokens = state
            .cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        tokens.insert(window_label.clone(), token.clone());
    }
    if mode == "test" {
        if let Some(ws) = workspace.as_ref().filter(|path| !path.trim().is_empty()) {
            state
                .test_workspaces
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(window_label.clone(), ws.clone());
        }
    }
    let result = skills::execute(
        &mode,
        &task,
        workspace.as_deref(),
        phase.as_deref(),
        context.as_deref(),
        issue.as_deref(),
        &config,
        &state.prompts,
        &window_label,
        &app_handle,
        token,
    )
    .await;
    // Remove token after run completes (cancelled or finished normally).
    state
        .cancel_tokens
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&window_label);
    if mode == "test" && (result.is_err() || phase.as_deref() == Some("document")) {
        let cleanup_workspace = state
            .test_workspaces
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&window_label);
        let _ = skills::test_skill::cleanup_runtime_for_window(
            &window_label,
            cleanup_workspace.as_deref(),
        );
    }
    match &result {
        Ok(()) => info!(mode = %mode, phase = ?phase, "skill completed"),
        Err(e) if e == "cancelled" => info!(mode = %mode, "skill cancelled"),
        Err(e) => warn!(mode = %mode, error = %e, "skill failed"),
    }
    result.map_err(|e| SkillError::from_raw(&e))
}

#[tauri::command]
fn cancel_skill(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>) {
    let window_label = window.label();
    let token = state
        .cancel_tokens
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(window_label)
        .cloned();
    if let Some(token) = token {
        token.cancel();
    }
    // Do NOT call cleanup_runtime_for_window here — let run_skill's
    // post-await cleanup handle it.  Calling it in both places causes a race
    // where cancel_skill removes the workspace from the map, then run_skill's
    // cleanup finds None and skips cleanup, or both run cleanup concurrently.
}

#[tauri::command]
fn open_new_window(app_handle: tauri::AppHandle) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static WINDOW_COUNTER: AtomicU64 = AtomicU64::new(0);
    let label = format!(
        "aidevchat-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        WINDOW_COUNTER.fetch_add(1, Ordering::Relaxed),
    );
    tauri::WebviewWindowBuilder::new(&app_handle, &label, tauri::WebviewUrl::App("/".into()))
        .title("FlowForge")
        .inner_size(1200.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .build()
        .map_err(|e| format!("Failed to create window: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn write_bug_report(
    task: String,
    issue: String,
    workspace: Option<String>,
) -> Result<String, String> {
    let path = if let Some(ws) = workspace.filter(|s| !s.is_empty()) {
        std::path::PathBuf::from(ws).join("bugs.md")
    } else {
        dirs::desktop_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
            .join("bugs.md")
    };
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = format!("\n## [{timestamp}] {task}\n\n**Issue:** {issue}\n\n---\n");
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Cannot open bugs.md: {e}"))?;
    file.write_all(entry.as_bytes())
        .map_err(|e| format!("Write error: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

// ── Memory commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn memory_load(workspace: Option<String>) -> Option<String> {
    memory::load_entrypoint(workspace.as_deref())
}

#[tauri::command]
fn memory_prompt(workspace: Option<String>, task_hint: String) -> Option<String> {
    memory::build_memory_prompt(workspace.as_deref(), &task_hint)
}

#[tauri::command]
fn memory_append(workspace: Option<String>, line: String) -> Result<String, String> {
    memory::append_to_entrypoint(workspace.as_deref(), &line)
}

#[tauri::command]
fn memory_write_topic(
    workspace: Option<String>,
    name: String,
    content: String,
) -> Result<String, String> {
    memory::write_topic(workspace.as_deref(), &name, &content)
}

#[tauri::command]
fn memory_list(workspace: Option<String>) -> Vec<String> {
    memory::list_memories(workspace.as_deref())
}

// ── Evidence commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn evidence_digest(workspace: String) -> Option<String> {
    skills::evidence::build_evidence_digest(&workspace)
}

#[tauri::command]
fn evidence_subtask_context(workspace: String, subtask_id: String) -> Option<String> {
    skills::evidence::build_subtask_context(&workspace, &subtask_id)
}

/// Test API connectivity by sending a minimal request to the configured endpoint.
/// Returns Ok(model_response_info) on success or Err(error_message) on failure.
///
/// Used by both Director and Agent identity cards in the settings UI —
/// accepts a provider name that is resolved via the provider registry.
#[tauri::command]
async fn test_agent_connection(
    provider: String,
    api_key: String,
    base_url: String,
    model: String,
) -> Result<String, String> {
    let provider = providers::ProviderConfig::from_fields(&provider, &api_key, &base_url, &model);
    send_test_request(&provider).await
}

#[tauri::command]
fn resolve_agent_provider(
    provider: String,
    api_key: String,
    base_url: String,
    model: String,
) -> ResolvedProviderInfo {
    providers::ProviderConfig::from_fields(&provider, &api_key, &base_url, &model)
        .to_resolved_info()
}

fn truncate_error(text: &str) -> String {
    if text.len() > 200 {
        // Find a char boundary at or before byte 200 to avoid panic on multi-byte UTF-8.
        let end = (0..=200)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        format!("{}...", &text[..end])
    } else {
        text.to_string()
    }
}

async fn send_test_request(provider: &providers::ProviderConfig) -> Result<String, String> {
    use reqwest::Client;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let body = serde_json::json!({
        "model": provider.model,
        "max_tokens": 8,
        "messages": [{"role": "user", "content": "Hi"}],
    });

    let request = match provider.wire {
        providers::WireFormat::Anthropic => client
            .post(format!(
                "{}/messages",
                provider.base_url.trim_end_matches('/')
            ))
            .header("x-api-key", &provider.api_key)
            .header("anthropic-version", "2023-06-01"),
        providers::WireFormat::OpenAI => client
            .post(format!(
                "{}/chat/completions",
                provider.base_url.trim_end_matches('/')
            ))
            .header("Authorization", format!("Bearer {}", provider.api_key)),
    };

    let resp = request
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("连接失败: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if status.is_success() {
        Ok(format!("连接成功 ({} {})", provider.name, provider.model))
    } else if status.as_u16() == 401 {
        Err("API Key 无效 (401 Unauthorized)".to_string())
    } else {
        Err(format!("API 返回 {status}: {}", truncate_error(&text)))
    }
}

// ── System tray ───────────────────────────────────────────────────────────────

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::TrayIconBuilder;
    use tauri::Manager;

    let show = MenuItemBuilder::with_id("show", "Show FlowForge").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;

    // Use a dedicated tray icon sized for macOS menu bar (44×44 @2x).
    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon@2x.png"))?;

    TrayIconBuilder::new()
        .icon(tray_icon)
        .tooltip("FlowForge")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    info!("FlowForge starting");

    let config = AppConfig::load();
    let prompts = Arc::new(Prompts::load());

    tauri::Builder::default()
        .manage(AppState {
            config: RwLock::new(config),
            prompts,
            histories: Mutex::new(HashMap::new()),
            cancel_tokens: Mutex::new(HashMap::new()),
            test_workspaces: Mutex::new(HashMap::new()),
        })
        // Backs the AskUserQuestion tool — pending replies are keyed by
        // request id and resolved through the `submit_user_answer` command.
        .manage(tools::ask_user_question::registry::UserQuestionRegistry::new())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_tools,
            get_config,
            get_config_form,
            save_config,
            set_execution_access_mode,
            director_chat,
            clear_history,
            get_director_history,
            restore_director_history,
            run_skill,
            cancel_skill,
            open_new_window,
            write_bug_report,
            workspace::create_workspace,
            workspace::workspace_tree,
            workspace::open_project,
            workspace::read_project_docs,
            workspace::read_workspace_file,
            history::save_session,
            history::list_sessions,
            history::load_session,
            history::delete_session,
            sanitize_blackboard_state,
            memory_load,
            memory_prompt,
            memory_append,
            memory_write_topic,
            memory_list,
            evidence_digest,
            evidence_subtask_context,
            test_agent_connection,
            resolve_agent_provider,
            tools::ask_user_question::registry::submit_user_answer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Initialize structured logging with tracing.
/// Logs go to both stderr (for development) and a rolling log file
/// in the app's data directory (~/.local/share/ai-dev-hub/logs/).
fn init_tracing() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("ai_dev_hub_lib=info,warn"));

    let stderr_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .compact();

    // Try to set up file logging; fall back silently if directory isn't available.
    let file_layer = dirs::data_dir().and_then(|data_dir| {
        let log_dir = data_dir.join("ai-dev-hub").join("logs");
        std::fs::create_dir_all(&log_dir).ok()?;
        let file_appender = tracing_appender::rolling::daily(&log_dir, "flowforge.log");
        Some(
            fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_target(true),
        )
    });

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init();
}
