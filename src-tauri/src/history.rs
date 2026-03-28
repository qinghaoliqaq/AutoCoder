/// Local chat history — saves/loads per-session JSON files.
///
/// Storage layout:
///   ~/.ai-dev-hub/sessions/<workspace-basename>/sess-<id>.json  (with workspace)
///   ~/.ai-dev-hub/sessions/sess-<id>.json                       (no workspace)

use crate::skills::BlackboardEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
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
    #[serde(default)]
    pub blackboard_events: Vec<BlackboardEvent>,
    #[serde(default)]
    pub project_context: Option<String>,
    #[serde(default)]
    pub project_context_source: Option<String>,
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

fn workspace_basename(workspace: &str) -> String {
    Path::new(workspace)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string())
}

fn workspace_hash(workspace: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in workspace.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn session_dir(workspace: Option<&str>) -> PathBuf {
    let root = sessions_root();
    match workspace.filter(|s| !s.trim().is_empty()) {
        Some(ws) => root.join(format!("{}-{:016x}", workspace_basename(ws), workspace_hash(ws))),
        None => root,
    }
}

fn legacy_session_dir(workspace: Option<&str>) -> Option<PathBuf> {
    workspace
        .filter(|s| !s.trim().is_empty())
        .map(|ws| sessions_root().join(workspace_basename(ws)))
}

fn session_dirs_for_read(workspace: Option<&str>) -> Vec<PathBuf> {
    let mut dirs = vec![session_dir(workspace)];
    if let Some(legacy) = legacy_session_dir(workspace) {
        if !dirs.iter().any(|dir| dir == &legacy) {
            dirs.push(legacy);
        }
    }
    dirs
}

fn session_paths_for_read(workspace: Option<&str>, session_id: &str) -> Vec<PathBuf> {
    session_dirs_for_read(workspace)
        .into_iter()
        .map(|dir| dir.join(format!("{session_id}.json")))
        .collect()
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
    let mut deduped: HashMap<String, SessionMeta> = HashMap::new();
    for dir in session_dirs_for_read(workspace.as_deref()) {
        if !dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("Read dir error: {e}"))?;
        for entry in entries.flatten() {
            if entry.path().extension().map(|x| x == "json").unwrap_or(false) {
                let Ok(text) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                let Ok(session) = serde_json::from_str::<SessionJson>(&text) else {
                    continue;
                };
                let meta = SessionMeta {
                    message_count: session.messages.len(),
                    ..session.meta
                };
                deduped
                    .entry(meta.id.clone())
                    .and_modify(|existing| {
                        if meta.updated_at > existing.updated_at {
                            *existing = meta.clone();
                        }
                    })
                    .or_insert(meta);
            }
        }
    }

    let mut metas: Vec<SessionMeta> = deduped.into_values().collect();

    // Most recently updated first
    metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(metas)
}

#[tauri::command]
pub fn load_session(workspace: Option<String>, session_id: String) -> Result<SessionJson, String> {
    for path in session_paths_for_read(workspace.as_deref(), &session_id) {
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("Cannot read session: {e}"))?;
        return serde_json::from_str(&text)
            .map_err(|e| format!("Parse error: {e}"));
    }
    Err(format!("Cannot read session: no session file found for {session_id}"))
}

#[tauri::command]
pub fn delete_session(workspace: Option<String>, session_id: String) -> Result<(), String> {
    for path in session_paths_for_read(workspace.as_deref(), &session_id) {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Delete error: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_dir_uses_workspace_hash() {
        let a = session_dir(Some("/tmp/demo/project"));
        let b = session_dir(Some("/opt/demo/project"));
        assert_ne!(a, b);
    }

    #[test]
    fn session_paths_for_read_include_legacy_location() {
        let paths = session_paths_for_read(Some("/tmp/demo/project"), "sess-1");
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("project-"));
        assert!(paths[1].to_string_lossy().ends_with("project/sess-1.json"));
    }
}
