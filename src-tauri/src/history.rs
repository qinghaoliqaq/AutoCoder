/// Local chat history — saves/loads per-session JSON files.
///
/// Storage layout:
///   ~/.ai-dev-hub/sessions/<workspace-basename>/sess-<id>.json  (with workspace)
///   ~/.ai-dev-hub/sessions/sess-<id>.json                       (no workspace)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionMeta {
    pub id:             String,
    pub title:          String,
    pub workspace_path: Option<String>,
    pub created_at:     u64,
    pub updated_at:     u64,
    pub message_count:  usize,
}

#[derive(Serialize, Deserialize)]
pub struct SessionJson {
    // Flatten so meta fields appear at top level in JSON
    #[serde(flatten)]
    pub meta:             SessionMeta,
    pub messages:         Vec<Value>,
    pub tool_logs:        Vec<Value>,
    /// Director's conversation history (OpenAI message format) — restored on load
    /// so the AI has full context of the previous conversation.
    #[serde(default)]
    pub director_history: Vec<Value>,
}

// ── Storage path helpers ───────────────────────────────────────────────────────

fn sessions_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".ai-dev-hub")
        .join("sessions")
}

fn session_dir(workspace: Option<&str>) -> PathBuf {
    let root = sessions_root();
    match workspace.filter(|s| !s.trim().is_empty()) {
        Some(ws) => {
            let name = Path::new(ws)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".to_string());
            root.join(name)
        }
        None => root,
    }
}

fn session_path(workspace: Option<&str>, session_id: &str) -> PathBuf {
    session_dir(workspace).join(format!("{session_id}.json"))
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_session(workspace: Option<String>, session: SessionJson) -> Result<(), String> {
    let dir = session_dir(workspace.as_deref());
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create sessions dir: {e}"))?;
    let path = dir.join(format!("{}.json", session.meta.id));
    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Write error: {e}"))
}

#[tauri::command]
pub fn list_sessions(workspace: Option<String>) -> Result<Vec<SessionMeta>, String> {
    let dir = session_dir(workspace.as_deref());
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut metas: Vec<SessionMeta> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Read dir error: {e}"))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().map(|x| x == "json").unwrap_or(false)
        })
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str::<SessionJson>(&text).ok())
        .map(|s| SessionMeta {
            message_count: s.messages.len(),
            ..s.meta
        })
        .collect();

    // Most recently updated first
    metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(metas)
}

#[tauri::command]
pub fn load_session(workspace: Option<String>, session_id: String) -> Result<SessionJson, String> {
    let path = session_path(workspace.as_deref(), &session_id);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read session: {e}"))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("Parse error: {e}"))
}

#[tauri::command]
pub fn delete_session(workspace: Option<String>, session_id: String) -> Result<(), String> {
    let path = session_path(workspace.as_deref(), &session_id);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Delete error: {e}"))?;
    }
    Ok(())
}
