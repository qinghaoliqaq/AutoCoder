mod config;
mod detect;
mod director;
pub(crate) mod errors;
mod evidence;
mod history;
pub(crate) mod memory;
mod planning_schema;
mod prompts;
mod skills;
pub(crate) mod tool_runner;
mod verifier;
mod workspace;

use config::{AppConfig, ConfigDraft, ConfigStatus, ExecutionAccessMode};
use detect::SystemStatus;
use director::chat_with_director;
use prompts::Prompts;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub prompts: Arc<Prompts>,
    pub histories: Mutex<HashMap<String, Vec<Value>>>,
    /// Per-window cancellation tokens for skill runs.
    pub cancel_tokens: Mutex<HashMap<String, CancellationToken>>,
    pub test_workspaces: Mutex<HashMap<String, String>>,
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

#[tauri::command]
fn detect_tools() -> SystemStatus {
    detect::detect_tools()
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> ConfigStatus {
    state.config.read().unwrap().status()
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
    *state.config.write().unwrap() = effective;
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
    *state.config.write().unwrap() = effective;
    Ok(status)
}

#[tauri::command]
async fn director_chat(
    input: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let config = state.config.read().unwrap().clone();
    let window_label = window.label().to_string();
    let token = CancellationToken::new();
    {
        let mut tokens = state.cancel_tokens.lock().unwrap();
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
    state.cancel_tokens.lock().unwrap().remove(&window_label);
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
) -> Result<(), String> {
    let config = state.config.read().unwrap().clone();
    let window_label = window.label().to_string();
    // Create a fresh cancellation token for this run, replacing any previous one.
    let token = CancellationToken::new();
    {
        let mut tokens = state.cancel_tokens.lock().unwrap();
        tokens.insert(window_label.clone(), token.clone());
    }
    if mode == "test" {
        if let Some(ws) = workspace.as_ref().filter(|path| !path.trim().is_empty()) {
            state
                .test_workspaces
                .lock()
                .unwrap()
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
    state.cancel_tokens.lock().unwrap().remove(&window_label);
    if mode == "test" && (result.is_err() || phase.as_deref() == Some("document")) {
        let cleanup_workspace = state.test_workspaces.lock().unwrap().remove(&window_label);
        let _ = skills::test_skill::cleanup_runtime_for_window(
            &window_label,
            cleanup_workspace.as_deref(),
        );
    }
    result
}

#[tauri::command]
fn cancel_skill(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>) {
    let window_label = window.label();
    if let Some(token) = state.cancel_tokens.lock().unwrap().get(window_label) {
        token.cancel();
    }
    skills::runners::kill_registered_processes(window_label);
    let cleanup_workspace = state.test_workspaces.lock().unwrap().remove(window_label);
    let _ =
        skills::test_skill::cleanup_runtime_for_window(window_label, cleanup_workspace.as_deref());
}

#[tauri::command]
fn open_new_window(app_handle: tauri::AppHandle) -> Result<(), String> {
    let label = format!(
        "aidevchat-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
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
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        std::path::PathBuf::from(home)
            .join("Desktop")
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
    evidence::build_evidence_digest(&workspace)
}

#[tauri::command]
fn evidence_subtask_context(workspace: String, subtask_id: String) -> Option<String> {
    evidence::build_subtask_context(&workspace, &subtask_id)
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
