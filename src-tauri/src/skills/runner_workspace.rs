/// Workspace snapshot and change-log tracking for CLI runners.
///
/// Records file-level changes (CREATE / MODIFY / DELETE) to `change.log`
/// in the workspace root, and provides before/after snapshot diffing so
/// runners can detect what a child process actually touched.
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

// ── change.log recording ──────────────────────────────────────────────────

/// Append a CREATE or MODIFY entry to <workspace>/change.log.
/// Called whenever Claude uses a file-writing tool (Write, Edit, Create, MultiEdit).
/// Silently ignores errors — change.log is best-effort.
pub(super) fn record_change(tool: &str, raw_json: &str, cwd: &PathBuf) {
    let file_path = if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        v["file_path"]
            .as_str()
            .or_else(|| v["path"].as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    let Some(file_path) = file_path else { return };
    // Resolve to absolute path
    let abs = if std::path::Path::new(&file_path).is_absolute() {
        PathBuf::from(&file_path)
    } else {
        cwd.join(&file_path)
    };
    let kind = match tool {
        "Write" | "Create" | "write_file" => "CREATE",
        _ => "MODIFY", // Edit, MultiEdit, etc.
    };
    let entry = format!("{kind}: {}\n", abs.to_string_lossy());
    let log_path = cwd.join(".ai-dev-hub/change.log");
    use std::io::Write as _;
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

// ── Workspace change types ────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceChangeKind {
    Create,
    Modify,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileFingerprint {
    pub len: u64,
    pub modified_unix_nanos: u128,
}

// ── change entry helpers ──────────────────────────────────────────────────

pub(super) fn append_change_entry(
    kind: WorkspaceChangeKind,
    path: &std::path::Path,
    cwd: &PathBuf,
) {
    let label = match kind {
        WorkspaceChangeKind::Create => "CREATE",
        WorkspaceChangeKind::Modify => "MODIFY",
        WorkspaceChangeKind::Delete => "DELETE",
    };
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let entry = format!("{label}: {}\n", abs.to_string_lossy());
    let log_path = cwd.join(".ai-dev-hub/change.log");
    use std::io::Write as _;
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

// ── Snapshot & diff ───────────────────────────────────────────────────────

pub(super) fn snapshot_workspace(root: &std::path::Path) -> HashMap<PathBuf, FileFingerprint> {
    let mut files = HashMap::new();
    collect_workspace_files(root, root, &mut files);
    files
}

fn collect_workspace_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    files: &mut HashMap<PathBuf, FileFingerprint>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() {
            if should_skip_workspace_dir(&name) {
                continue;
            }
            collect_workspace_files(root, &path, files);
            continue;
        }

        if !path.is_file() || should_skip_workspace_file(&name) {
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified_unix_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        files.insert(
            relative.to_path_buf(),
            FileFingerprint {
                len: metadata.len(),
                modified_unix_nanos,
            },
        );
    }
}

fn should_skip_workspace_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "__pycache__" | "target" | "dist" | ".next"
        )
}

fn should_skip_workspace_file(name: &str) -> bool {
    // change.log now lives in .ai-dev-hub/ which is skipped as a directory
    false
}

pub(super) fn workspace_change_entries(
    before: &HashMap<PathBuf, FileFingerprint>,
    after: &HashMap<PathBuf, FileFingerprint>,
) -> Vec<(WorkspaceChangeKind, PathBuf)> {
    let mut entries = Vec::new();

    for (path, fingerprint) in after {
        match before.get(path) {
            None => entries.push((WorkspaceChangeKind::Create, path.clone())),
            Some(previous) if previous != fingerprint => {
                entries.push((WorkspaceChangeKind::Modify, path.clone()))
            }
            Some(_) => {}
        }
    }

    for path in before.keys() {
        if !after.contains_key(path) {
            entries.push((WorkspaceChangeKind::Delete, path.clone()));
        }
    }

    entries.sort_by(|a, b| a.1.cmp(&b.1));
    entries
}

pub(super) fn record_workspace_snapshot_diff(
    cwd: &PathBuf,
    before: &HashMap<PathBuf, FileFingerprint>,
    after: &HashMap<PathBuf, FileFingerprint>,
) {
    for (kind, path) in workspace_change_entries(before, after) {
        append_change_entry(kind, &path, cwd);
    }
}

pub(super) fn format_workspace_change_list(changes: &[(WorkspaceChangeKind, PathBuf)]) -> String {
    changes
        .iter()
        .map(|(kind, path)| {
            let label = match kind {
                WorkspaceChangeKind::Create => "CREATE",
                WorkspaceChangeKind::Modify => "MODIFY",
                WorkspaceChangeKind::Delete => "DELETE",
            };
            format!("{label}: {}", path.display())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_change_entries_detect_create_and_modify() {
        let mut before = HashMap::new();
        before.insert(
            PathBuf::from("src/lib.rs"),
            FileFingerprint {
                len: 10,
                modified_unix_nanos: 1,
            },
        );

        let mut after = HashMap::new();
        after.insert(
            PathBuf::from("src/lib.rs"),
            FileFingerprint {
                len: 12,
                modified_unix_nanos: 2,
            },
        );
        after.insert(
            PathBuf::from("src/new.rs"),
            FileFingerprint {
                len: 5,
                modified_unix_nanos: 2,
            },
        );

        assert_eq!(
            workspace_change_entries(&before, &after),
            vec![
                (WorkspaceChangeKind::Modify, PathBuf::from("src/lib.rs")),
                (WorkspaceChangeKind::Create, PathBuf::from("src/new.rs")),
            ]
        );
    }

    #[test]
    fn workspace_change_entries_detect_delete() {
        let mut before = HashMap::new();
        before.insert(
            PathBuf::from("src/old.rs"),
            FileFingerprint {
                len: 10,
                modified_unix_nanos: 1,
            },
        );

        let after = HashMap::new();

        assert_eq!(
            workspace_change_entries(&before, &after),
            vec![(WorkspaceChangeKind::Delete, PathBuf::from("src/old.rs"))]
        );
    }

    #[test]
    fn snapshot_workspace_skips_ai_dev_hub_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn demo() {}").unwrap();
        std::fs::write(dir.path().join(".ai-dev-hub/change.log"), "noise").unwrap();

        let snapshot = snapshot_workspace(dir.path());
        assert!(snapshot.contains_key(&PathBuf::from("src/lib.rs")));
        assert!(!snapshot.contains_key(&PathBuf::from(".ai-dev-hub/change.log")));
    }

    #[test]
    fn record_change_writes_create_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let json = r#"{"file_path":"src/main.rs"}"#;
        record_change("Write", json, &cwd);
        let log = std::fs::read_to_string(cwd.join(".ai-dev-hub/change.log")).unwrap();
        assert!(log.contains("CREATE:"));
        assert!(log.contains("src/main.rs"));
    }

    #[test]
    fn record_change_writes_modify_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let json = r#"{"file_path":"src/lib.rs"}"#;
        record_change("Edit", json, &cwd);
        let log = std::fs::read_to_string(cwd.join(".ai-dev-hub/change.log")).unwrap();
        assert!(log.contains("MODIFY:"));
    }

    #[test]
    fn record_change_ignores_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        record_change("Write", "not-json", &cwd);
        assert!(!cwd.join(".ai-dev-hub/change.log").exists());
    }

    #[test]
    fn record_change_appends_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        record_change("Write", r#"{"file_path":"a.rs"}"#, &cwd);
        record_change("Edit", r#"{"file_path":"b.rs"}"#, &cwd);
        let log = std::fs::read_to_string(cwd.join(".ai-dev-hub/change.log")).unwrap();
        assert!(log.contains("a.rs"));
        assert!(log.contains("b.rs"));
        assert_eq!(log.lines().count(), 2);
    }

    #[test]
    fn format_workspace_change_list_renders_kinds_and_paths() {
        let rendered = format_workspace_change_list(&[
            (WorkspaceChangeKind::Create, PathBuf::from("src/new.rs")),
            (WorkspaceChangeKind::Modify, PathBuf::from("src/lib.rs")),
            (WorkspaceChangeKind::Delete, PathBuf::from("src/old.rs")),
        ]);
        assert_eq!(
            rendered,
            "CREATE: src/new.rs, MODIFY: src/lib.rs, DELETE: src/old.rs"
        );
    }
}
