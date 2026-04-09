/// Isolated workspace management for parallel subtask execution.
///
/// Provides fork/sync/cleanup of workspaces so each subtask works on its own
/// copy without interfering with others or the main workspace.
use super::blackboard::{BLACKBOARD_JSON, BLACKBOARD_MD};
use super::verifier::VERIFIER_RESULT_JSON;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const PLAN_MD: &str = ".ai-dev-hub/PLAN.md";
const PLAN_BOARD_MD: &str = ".ai-dev-hub/PLAN_BLACKBOARD.md";
const PLAN_BOARD_JSON: &str = ".ai-dev-hub/PLAN_BLACKBOARD.json";
const PLAN_GRAPH_JSON: &str = ".ai-dev-hub/PLAN_GRAPH.json";
pub(crate) const SCRATCH_ROOT_DIR: &str = ".ai-dev-hub/subtasks";

// ── Data types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileFingerprint {
    pub len: u64,
    pub modified_unix_nanos: u128,
}

#[derive(Clone, Debug)]
pub(crate) struct IsolatedWorkspace {
    pub root: PathBuf,
    /// Read-only copy of the workspace at fork time, used as the common
    /// ancestor for three-way merges when parallel subtasks modify the same file.
    pub base_dir: PathBuf,
    pub base_snapshot: HashMap<PathBuf, FileFingerprint>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceChanges {
    pub changed_or_created: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}

// ── Create / cleanup ───────────────────────────────────────────────────────

pub(crate) fn create_isolated_workspace(
    workspace: &str,
    subtask_id: &str,
    attempt: u32,
) -> Result<IsolatedWorkspace, String> {
    let workspace_root = Path::new(workspace);
    let scratch_root = workspace_root.join(SCRATCH_ROOT_DIR).join(subtask_id);
    let attempt_root = scratch_root.join(format!("attempt-{attempt}"));

    if attempt_root.exists() {
        std::fs::remove_dir_all(&attempt_root).map_err(|e| {
            format!(
                "Cannot reset isolated workspace {}: {e}",
                attempt_root.display()
            )
        })?;
    }
    std::fs::create_dir_all(&attempt_root).map_err(|e| {
        format!(
            "Cannot create isolated workspace {}: {e}",
            attempt_root.display()
        )
    })?;

    copy_workspace_tree(workspace_root, &attempt_root, workspace_root)?;
    let base_snapshot = snapshot_workspace(&attempt_root);

    // Keep a frozen copy of the workspace as-is at fork time so we can use it
    // as the common ancestor for three-way merges.
    // IMPORTANT: Copy from attempt_root (already forked) instead of from
    // workspace_root again — otherwise a concurrent merge between the two
    // copy_workspace_tree calls would make the base out of sync with the
    // working copy (TOCTOU race).
    let base_dir = scratch_root.join(format!("base-{attempt}"));
    if base_dir.exists() {
        std::fs::remove_dir_all(&base_dir).ok();
    }
    std::fs::create_dir_all(&base_dir).map_err(|e| {
        format!("Cannot create base dir {}: {e}", base_dir.display())
    })?;
    copy_workspace_tree(&attempt_root, &base_dir, &attempt_root)?;

    Ok(IsolatedWorkspace {
        root: attempt_root,
        base_dir,
        base_snapshot,
    })
}

/// Remove all isolated workspace directories that are not referenced by the
/// blackboard's `isolated_workspace` fields.  Called at startup to reclaim
/// disk space from previous runs that leaked workspaces (crash, retry→error
/// transitions, etc.).
///
/// This is conservative: it only removes directories under
/// `.ai-dev-hub/subtasks/` — it never touches the main workspace.
pub(crate) fn cleanup_orphaned_workspaces(workspace: &str, active_roots: &[String]) {
    let scratch = Path::new(workspace).join(SCRATCH_ROOT_DIR);
    if !scratch.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&scratch) else {
        return;
    };

    let active_set: std::collections::HashSet<&str> =
        active_roots.iter().map(|s| s.as_str()).collect();
    let mut cleaned = 0u64;

    for entry in entries.flatten() {
        let subtask_dir = entry.path();
        if !subtask_dir.is_dir() {
            continue;
        }
        // Check each attempt-N and base-N directory inside the subtask folder.
        let Ok(sub_entries) = std::fs::read_dir(&subtask_dir) else {
            continue;
        };
        for sub_entry in sub_entries.flatten() {
            let path = sub_entry.path();
            if !path.is_dir() {
                // Skip files (e.g., merge-journal.json — handled by recovery).
                continue;
            }
            let path_str = path.to_string_lossy().to_string();
            if active_set.contains(path_str.as_str()) {
                continue; // This workspace is still active — don't delete.
            }
            let name = sub_entry.file_name().to_string_lossy().to_string();
            // Protect base-N dirs whose matching attempt-N is active.
            // The blackboard only stores the attempt root, not the base dir,
            // so we must infer protection for base dirs here.
            if name.starts_with("base-") {
                let matching_attempt = subtask_dir
                    .join(name.replacen("base-", "attempt-", 1))
                    .to_string_lossy()
                    .to_string();
                if active_set.contains(matching_attempt.as_str()) {
                    continue; // Corresponding attempt is active — protect the base.
                }
            }
            // Only clean up attempt-N, base-N, and merge-staging directories.
            if name.starts_with("attempt-")
                || name.starts_with("base-")
                || name == "merge-staging"
            {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if meta.is_dir() {
                        let size = dir_size_approx(&path);
                        if std::fs::remove_dir_all(&path).is_ok() {
                            cleaned += size;
                        }
                    }
                }
            }
        }
        // If the subtask dir is now empty, remove it too.
        if let Ok(mut remaining) = std::fs::read_dir(&subtask_dir) {
            if remaining.next().is_none() {
                let _ = std::fs::remove_dir(&subtask_dir);
            }
        }
    }

    if cleaned > 0 {
        let cleaned_mb = cleaned / (1024 * 1024);
        tracing::info!("Cleaned up ~{cleaned_mb}MB of orphaned workspace data");
    }
}

/// Quick approximate size of a directory tree (best-effort, no errors).
fn dir_size_approx(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size_approx(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

pub(crate) fn cleanup_isolated_workspace(root: &Path) -> Result<(), String> {
    if root.exists() {
        std::fs::remove_dir_all(root)
            .map_err(|e| format!("Cannot remove isolated workspace {}: {e}", root.display()))?;
    }

    let mut current = root.parent();
    for _ in 0..2 {
        let Some(dir) = current else {
            break;
        };
        // Parent may already be gone (e.g. cleaned up by a prior call).
        let is_empty = match std::fs::read_dir(dir) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => break,
        };
        if !is_empty {
            break;
        }
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }

    Ok(())
}

// ── Sync coordination files ────────────────────────────────────────────────

pub(crate) fn sync_coordination_files(
    main_workspace: &str,
    isolated_workspace: &Path,
) -> Result<(), String> {
    // BLACKBOARD.json is the only strictly required file — without it the
    // subtask has no context.  All other coordination files are supplementary
    // (PLAN.md context, markdown renderings).  We copy BLACKBOARD.json with
    // error propagation and treat the rest as best-effort, because concurrent
    // persist()/tick_plan_checkbox() calls can make any of these files
    // transiently unreadable.
    let critical = [BLACKBOARD_JSON];
    let supplementary = [PLAN_MD, PLAN_BOARD_MD, PLAN_BOARD_JSON, PLAN_GRAPH_JSON, BLACKBOARD_MD];

    for relative in critical {
        let source = Path::new(main_workspace).join(relative);
        if !source.exists() {
            continue;
        }
        let target = isolated_workspace.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&source, &target).map_err(|e| {
            format!("Cannot sync {} -> {}: {e}", source.display(), target.display())
        })?;
    }

    for relative in supplementary {
        let source = Path::new(main_workspace).join(relative);
        if !source.exists() {
            continue;
        }
        let target = isolated_workspace.join(relative);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&source, &target) {
            tracing::warn!("Failed to sync {relative} to isolated workspace (non-fatal): {e}");
        }
    }
    Ok(())
}

// ── Copy workspace tree ────────────────────────────────────────────────────

fn copy_workspace_tree(
    source_root: &Path,
    target_root: &Path,
    current: &Path,
) -> Result<(), String> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| format!("Cannot read {}: {e}", current.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = path
            .strip_prefix(source_root)
            .map_err(|e| format!("Cannot relativize {}: {e}", path.display()))?;
        let target = target_root.join(relative);

        // Use entry.file_type() (lstat) instead of path.is_dir()/is_file()
        // to avoid following symlinks, which could escape the workspace sandbox.
        let ft = entry.file_type()
            .map_err(|e| format!("Cannot stat {}: {e}", path.display()))?;

        if ft.is_symlink() {
            continue; // skip symlinks — never follow them outside the workspace
        }

        if ft.is_dir() {
            if should_skip_workspace_dir(&name) {
                continue;
            }
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("Cannot create {}: {e}", target.display()))?;
            copy_workspace_tree(source_root, target_root, &path)?;
            continue;
        }

        if !ft.is_file() || should_skip_workspace_file(&name) {
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&path, &target).map_err(|e| {
            format!(
                "Cannot copy {} -> {}: {e}",
                path.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

// ── Snapshot & diff ────────────────────────────────────────────────────────

pub(crate) fn snapshot_workspace(root: &Path) -> HashMap<PathBuf, FileFingerprint> {
    let mut files = HashMap::new();
    collect_workspace_files(root, root, &mut files);
    files
}

fn collect_workspace_files(root: &Path, dir: &Path, files: &mut HashMap<PathBuf, FileFingerprint>) {
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

pub(crate) fn workspace_changes(
    before: &HashMap<PathBuf, FileFingerprint>,
    after: &HashMap<PathBuf, FileFingerprint>,
) -> WorkspaceChanges {
    let mut changed_or_created = Vec::new();
    let mut deleted = Vec::new();

    for (path, fingerprint) in after {
        match before.get(path) {
            None => changed_or_created.push(path.clone()),
            Some(previous) if previous != fingerprint => changed_or_created.push(path.clone()),
            Some(_) => {}
        }
    }

    for path in before.keys() {
        if !after.contains_key(path) {
            deleted.push(path.clone());
        }
    }

    changed_or_created.sort();
    deleted.sort();
    WorkspaceChanges {
        changed_or_created,
        deleted,
    }
}

pub(crate) fn relative_paths_from_root(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut relative = Vec::new();

    for path in paths {
        let display = root
            .join(path)
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        if seen.insert(display.clone()) {
            relative.push(display);
        }
    }

    relative
}

// ── Filters ────────────────────────────────────────────────────────────────

pub(crate) fn should_skip_workspace_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".ai-dev-hub"
            | "node_modules"
            | "__pycache__"
            | "target"
            | "dist"
            | "build"
            | "out"
            | ".next"
            | ".turbo"
            | ".cache"
            | ".aws"
            | ".ssh"
            | ".gnupg"
    )
}

pub(crate) fn should_skip_workspace_file(name: &str) -> bool {
    // Orchestration files (BLACKBOARD, PLAN, change.log, etc.) now live inside
    // .ai-dev-hub/ which is already skipped by should_skip_workspace_dir.
    // Only verifier-result.json remains at the isolated workspace root level.
    if name == VERIFIER_RESULT_JSON {
        return true;
    }

    let lower = name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || matches!(
            lower.as_str(),
            ".pypirc"
                | ".netrc"
                | "service-account.json"
                | "credentials.json"
        )
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
        || lower.ends_with(".crt")
        || lower.ends_with(".cer")
        || lower.ends_with(".der")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_changes_detects_create_modify_delete() {
        let before = HashMap::from([
            (
                PathBuf::from("a.txt"),
                FileFingerprint {
                    len: 1,
                    modified_unix_nanos: 1,
                },
            ),
            (
                PathBuf::from("b.txt"),
                FileFingerprint {
                    len: 1,
                    modified_unix_nanos: 1,
                },
            ),
        ]);
        let after = HashMap::from([
            (
                PathBuf::from("a.txt"),
                FileFingerprint {
                    len: 2,
                    modified_unix_nanos: 2,
                },
            ),
            (
                PathBuf::from("c.txt"),
                FileFingerprint {
                    len: 3,
                    modified_unix_nanos: 3,
                },
            ),
        ]);

        let changes = workspace_changes(&before, &after);
        assert_eq!(
            changes.changed_or_created,
            vec![PathBuf::from("a.txt"), PathBuf::from("c.txt")]
        );
        assert_eq!(changes.deleted, vec![PathBuf::from("b.txt")]);
    }

    #[test]
    fn relative_paths_from_root_dedupes() {
        let root = Path::new("/tmp/demo");
        let paths = vec![PathBuf::from("src/app.ts"), PathBuf::from("src/app.ts")];
        let result = relative_paths_from_root(root, &paths);
        assert_eq!(result, vec!["src/app.ts".to_string()]);
    }

    #[test]
    fn should_skip_verifier_result_artifact() {
        assert!(should_skip_workspace_file(VERIFIER_RESULT_JSON));
    }
}
