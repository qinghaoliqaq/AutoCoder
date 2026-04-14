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
    pub id: String,
    pub title: String,
    pub workspace_path: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SessionJson {
    // Flatten so meta fields appear at top level in JSON
    #[serde(flatten)]
    pub meta: SessionMeta,
    pub messages: Vec<Value>,
    pub tool_logs: Vec<Value>,
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
        Some(ws) => root.join(format!(
            "{}-{:016x}",
            workspace_basename(ws),
            workspace_hash(ws)
        )),
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

fn validate_session_id(session_id: &str) -> Result<&str, String> {
    if session_id.is_empty() {
        return Err("Invalid session id: empty".to_string());
    }
    if session_id.len() > 128 {
        return Err("Invalid session id: too long".to_string());
    }
    if !session_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(format!("Invalid session id: {session_id}"));
    }
    Ok(session_id)
}

fn session_file_name(session_id: &str) -> Result<String, String> {
    Ok(format!("{}.json", validate_session_id(session_id)?))
}

fn session_paths_for_read(
    workspace: Option<&str>,
    session_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let file_name = session_file_name(session_id)?;
    Ok(session_dirs_for_read(workspace)
        .into_iter()
        .map(|dir| dir.join(&file_name))
        .collect())
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_session(workspace: Option<String>, session: SessionJson) -> Result<(), String> {
    let dir = session_dir(workspace.as_deref());
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create sessions dir: {e}"))?;
    let path = dir.join(session_file_name(&session.meta.id)?);
    let json =
        serde_json::to_string_pretty(&session).map_err(|e| format!("Serialize error: {e}"))?;
    // Tmp name must be unique: two parallel `save_session` calls for the
    // same session id would otherwise both write to `<id>.json.tmp`,
    // producing a corrupt mixed-content file when the later writer
    // lands while the earlier rename is in flight.
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = path.with_extension(format!("json.{pid}.{ns}.tmp"));
    std::fs::write(&tmp_path, &json).map_err(|e| format!("Write error: {e}"))?;
    // On Windows, rename fails if the destination exists; remove it first.
    #[cfg(target_os = "windows")]
    {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp_path, &path).map_err(|e| {
        // Remove the orphan tmp so it can't accumulate across retries.
        let _ = std::fs::remove_file(&tmp_path);
        format!("Rename error: {e}")
    })
}

/// Lightweight struct for extracting only metadata from session files without
/// deserializing the full messages/tool_logs/director_history arrays.
#[derive(Deserialize)]
struct SessionMetaOnly {
    #[serde(flatten)]
    meta: SessionMeta,
    /// We only need the length, but serde requires us to declare the field.
    /// Using `Value` for each element avoids parsing the inner structure and
    /// `default` means missing field = empty vec.
    #[serde(default)]
    messages: Vec<Box<serde_json::value::RawValue>>,
}

#[tauri::command]
pub fn list_sessions(workspace: Option<String>) -> Result<Vec<SessionMeta>, String> {
    let mut deduped: HashMap<String, SessionMeta> = HashMap::new();
    for dir in session_dirs_for_read(workspace.as_deref()) {
        if !dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("Read dir error: {e}"))?;
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .map(|x| x == "json")
                .unwrap_or(false)
            {
                let Ok(text) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                // Use SessionMetaOnly to avoid fully deserializing the large
                // messages/tool_logs/director_history arrays.  RawValue skips
                // parsing while still counting the array length.
                let Ok(partial) = serde_json::from_str::<SessionMetaOnly>(&text) else {
                    continue;
                };
                if validate_session_id(&partial.meta.id).is_err() {
                    continue;
                }
                let meta = SessionMeta {
                    message_count: partial.messages.len(),
                    ..partial.meta
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
    for path in session_paths_for_read(workspace.as_deref(), &session_id)? {
        if !path.exists() {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("Cannot read session: {e}"))?;
        return serde_json::from_str(&text).map_err(|e| format!("Parse error: {e}"));
    }
    Err(format!(
        "Cannot read session: no session file found for {session_id}"
    ))
}

#[tauri::command]
pub fn delete_session(workspace: Option<String>, session_id: String) -> Result<(), String> {
    for path in session_paths_for_read(workspace.as_deref(), &session_id)? {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Delete error: {e}"))?;
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
        let paths = session_paths_for_read(Some("/tmp/demo/project"), "sess-1").unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("project-"));
        assert!(paths[1].to_string_lossy().ends_with("project/sess-1.json"));
    }

    #[test]
    fn validate_session_id_rejects_path_traversal() {
        assert!(validate_session_id("../escape").is_err());
        assert!(validate_session_id("sess/../../x").is_err());
        assert!(validate_session_id("sess.with.dot").is_err());
    }

    #[test]
    fn validate_session_id_accepts_generated_format() {
        assert_eq!(validate_session_id("sess-123_abc").unwrap(), "sess-123_abc");
    }
}
