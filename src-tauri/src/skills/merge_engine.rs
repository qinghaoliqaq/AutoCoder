/// Line-level three-way merge engine for parallel subtask workspaces.
///
/// When multiple subtasks modify files in parallel, the merge engine detects
/// conflicts and performs automatic three-way merges where possible.
///
/// Merges use a **staged journal** pattern for crash safety:
///   1. All merge results are written to a staging directory first.
///   2. A `merge-journal.json` records the planned file operations.
///   3. Files are copied from staging to the main workspace.
///   4. The journal is removed on completion.
///
/// On recovery (`recover_pending_merges`), pending journals are detected
/// and completed — preventing the double-merge corruption that would
/// otherwise occur when a subtask re-implements on an already-merged workspace.
use super::isolated_workspace::{snapshot_workspace, workspace_changes, IsolatedWorkspace, SCRATCH_ROOT_DIR};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ── Merge journal for crash safety ─────────────────────────────────────────

const MERGE_JOURNAL: &str = "merge-journal.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MergeJournal {
    subtask_id: String,
    /// Files to copy from staging to main workspace (relative paths).
    copy_files: Vec<String>,
    /// Files to delete from main workspace (relative paths).
    delete_files: Vec<String>,
    /// Staging directory containing the pre-built merge results.
    staging_dir: String,
}

/// Check for and complete any pending merges left by a crash.
///
/// Called at the start of `run()` and from `sanitize_blackboard_state`.
/// If a merge journal exists, the merge was interrupted — we complete it
/// by copying any remaining staged files to the main workspace.
/// Returns the list of subtask IDs that were recovered.
pub(crate) fn recover_pending_merges(workspace: &str) -> Vec<String> {
    let scratch = Path::new(workspace).join(SCRATCH_ROOT_DIR);
    let Ok(subtask_dirs) = std::fs::read_dir(&scratch) else {
        return Vec::new();
    };

    let mut recovered = Vec::new();

    for entry in subtask_dirs.flatten() {
        let subtask_dir = entry.path();
        if !subtask_dir.is_dir() {
            continue;
        }
        let journal_path = subtask_dir.join(MERGE_JOURNAL);
        if !journal_path.exists() {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&journal_path) else {
            tracing::warn!("Cannot read merge journal {}", journal_path.display());
            continue;
        };
        let Ok(journal) = serde_json::from_str::<MergeJournal>(&text) else {
            tracing::warn!("Cannot parse merge journal {}", journal_path.display());
            // Remove corrupt journal so it doesn't block future runs.
            let _ = std::fs::remove_file(&journal_path);
            continue;
        };

        tracing::info!(
            subtask = %journal.subtask_id,
            files = journal.copy_files.len(),
            "Recovering interrupted merge from journal"
        );

        let main_root = Path::new(workspace);
        // Derive staging dir from known structure instead of trusting the
        // journal's staging_dir field (defense against tampered journals).
        let staging = subtask_dir.join("merge-staging");
        if !staging.exists() {
            tracing::warn!("Staging dir missing for journal {} — removing stale journal", journal_path.display());
            let _ = std::fs::remove_file(&journal_path);
            continue;
        }

        // Complete the merge: copy remaining staged files to main workspace.
        for relative in &journal.copy_files {
            // Path traversal protection: reject paths with ".." or absolute paths.
            if relative.contains("..") || Path::new(relative).is_absolute() {
                tracing::warn!("Skipping suspicious path in merge journal: {relative}");
                continue;
            }
            let staged_file = staging.join(relative);
            let target = main_root.join(relative);
            if !staged_file.exists() {
                // Already applied or staging was incomplete — skip.
                continue;
            }
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::copy(&staged_file, &target) {
                tracing::warn!(
                    "Failed to recover merge file {} -> {}: {e}",
                    staged_file.display(),
                    target.display()
                );
            }
        }

        // Complete deletes.
        for relative in &journal.delete_files {
            if relative.contains("..") || Path::new(relative).is_absolute() {
                tracing::warn!("Skipping suspicious delete path in merge journal: {relative}");
                continue;
            }
            let target = main_root.join(relative);
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
        }

        // Journal complete — remove it and the staging directory.
        let _ = std::fs::remove_file(&journal_path);
        if staging.exists() {
            let _ = std::fs::remove_dir_all(staging);
        }

        recovered.push(journal.subtask_id);
    }

    recovered
}

// ── Main merge entry point ─────────────────────────────────────────────────

pub(crate) fn merge_isolated_workspace(
    workspace: &str,
    isolated: &IsolatedWorkspace,
) -> Result<Vec<String>, String> {
    let main_root = Path::new(workspace);
    let isolated_after = snapshot_workspace(&isolated.root);
    let changes = workspace_changes(&isolated.base_snapshot, &isolated_after);
    let main_before = snapshot_workspace(main_root);

    tracing::info!(
        isolated_root = %isolated.root.display(),
        main_root = %main_root.display(),
        base_snapshot_files = isolated.base_snapshot.len(),
        isolated_after_files = isolated_after.len(),
        changed_or_created = changes.changed_or_created.len(),
        deleted = changes.deleted.len(),
        "Merging isolated workspace back to main"
    );

    let mut conflicts = Vec::new();
    let mut touched = Vec::new();
    // Files that diverged in main since we forked — need three-way merge.
    let mut needs_merge: Vec<PathBuf> = Vec::new();

    for path in &changes.changed_or_created {
        let base = isolated.base_snapshot.get(path);
        let main = main_before.get(path);
        if main != base {
            needs_merge.push(path.clone());
        }
    }

    for path in &changes.deleted {
        let base = isolated.base_snapshot.get(path);
        let main = main_before.get(path);
        if main != base {
            // If main also deleted the file (main is None, base is Some),
            // both sides agree — no conflict.
            if main.is_none() && base.is_some() {
                continue;
            }
            // Cannot three-way merge a delete vs modify — true conflict.
            conflicts.push(path.to_string_lossy().into_owned());
        }
    }

    // Attempt line-level three-way merge for diverged text files.
    let mut merge_results: Vec<(PathBuf, String)> = Vec::new();
    for path in &needs_merge {
        // Three versions: base (frozen at fork time), main (current workspace),
        // ours (isolated workspace after subtask edits).
        let base_content = read_base_content(&isolated.base_dir, path);
        // Binary files (non-UTF8) cannot be three-way merged — treat as conflict.
        let main_content = match std::fs::read_to_string(main_root.join(path)) {
            Ok(s) => s,
            Err(_) => {
                conflicts.push(path.to_string_lossy().into_owned());
                continue;
            }
        };
        let ours_content = match std::fs::read_to_string(isolated.root.join(path)) {
            Ok(s) => s,
            Err(_) => {
                conflicts.push(path.to_string_lossy().into_owned());
                continue;
            }
        };

        match three_way_merge(&base_content, &main_content, &ours_content) {
            Ok(merged) => merge_results.push((path.clone(), merged)),
            Err(_) => conflicts.push(path.to_string_lossy().into_owned()),
        }
    }

    if !conflicts.is_empty() {
        conflicts.sort();
        conflicts.dedup();
        // Include actual diff context so Claude can restructure code to avoid
        // conflicting regions instead of blindly retrying.
        let mut detail = format!(
            "Merge conflict on files already changed in the main workspace: {}\n\n",
            conflicts.join(", ")
        );
        for conflict_path in &conflicts {
            let path = Path::new(conflict_path);
            let base_content = read_base_content(&isolated.base_dir, path);
            let main_content =
                std::fs::read_to_string(main_root.join(path)).unwrap_or_default();
            let ours_content =
                std::fs::read_to_string(isolated.root.join(path)).unwrap_or_default();
            detail.push_str(&format!("--- {conflict_path} ---\n"));
            detail.push_str(&summarize_conflict(&base_content, &main_content, &ours_content));
            detail.push('\n');
        }
        return Err(detail);
    }

    // ── Stage all merge results before touching main workspace ────────
    //
    // Write merged/copied files to a staging directory first.  Then write
    // a merge journal listing all planned operations.  Finally apply from
    // staging to main.  If the process crashes during apply, recovery can
    // complete the remaining copies from the journal on next startup.

    // Derive subtask scratch dir from the isolated root path
    // (e.g., .../subtasks/F1/attempt-1 → .../subtasks/F1/).
    let subtask_scratch = isolated
        .root
        .parent()
        .unwrap_or(&isolated.root);
    let staging_dir = subtask_scratch.join("merge-staging");
    if staging_dir.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
    }
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Cannot create staging dir {}: {e}", staging_dir.display()))?;

    // Stage three-way merge results.
    for (path, merged_content) in &merge_results {
        let staged = staging_dir.join(path);
        if let Some(parent) = staged.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create staging subdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&staged, merged_content)
            .map_err(|e| format!("Cannot stage merged {}: {e}", staged.display()))?;
        touched.push(path.to_string_lossy().into_owned());
    }

    // Stage files that only we changed (no divergence in main).
    let merge_set: HashSet<&PathBuf> = needs_merge.iter().collect();
    for path in &changes.changed_or_created {
        if merge_set.contains(path) {
            continue; // Already staged above.
        }
        let source = isolated.root.join(path);
        let staged = staging_dir.join(path);
        if let Some(parent) = staged.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create staging subdir {}: {e}", parent.display()))?;
        }
        std::fs::copy(&source, &staged).map_err(|e| {
            format!("Cannot stage {} -> {}: {e}", source.display(), staged.display())
        })?;
        touched.push(path.to_string_lossy().into_owned());
    }

    let delete_files: Vec<String> = changes
        .deleted
        .iter()
        .filter(|path| {
            // Only delete if main hasn't diverged (conflicts already handled).
            let base = isolated.base_snapshot.get(*path);
            let main = main_before.get(*path);
            main == base
        })
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    touched.sort();
    touched.dedup();

    // Derive subtask_id from the scratch dir name.
    let subtask_id = subtask_scratch
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    // Write the merge journal BEFORE applying — this is the crash-safety
    // invariant.  If we crash after this point, recovery will complete
    // the merge from the staged files.
    let journal = MergeJournal {
        subtask_id: subtask_id.clone(),
        copy_files: touched.clone(),
        delete_files: delete_files.clone(),
        staging_dir: staging_dir.to_string_lossy().into_owned(),
    };
    let journal_path = subtask_scratch.join(MERGE_JOURNAL);
    let journal_json = serde_json::to_string_pretty(&journal)
        .map_err(|e| format!("Cannot serialize merge journal: {e}"))?;
    // Atomic write: tmp + rename to prevent corrupt journal on crash.
    let journal_tmp = journal_path.with_extension("tmp");
    std::fs::write(&journal_tmp, &journal_json)
        .map_err(|e| format!("Cannot write merge journal tmp {}: {e}", journal_tmp.display()))?;
    std::fs::rename(&journal_tmp, &journal_path)
        .map_err(|e| format!("Cannot rename merge journal {}: {e}", journal_path.display()))?;

    // ── Apply from staging to main workspace ───────────────────────────
    for relative in &touched {
        let staged = staging_dir.join(relative);
        let target = main_root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&staged, &target).map_err(|e| {
            format!(
                "Cannot apply merge {} -> {}: {e}",
                staged.display(),
                target.display()
            )
        })?;
    }

    for relative in &delete_files {
        let target = main_root.join(relative);
        if target.exists() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("Cannot remove {} during merge: {e}", target.display()))?;
        }
        if !touched.contains(relative) {
            touched.push(relative.clone());
        }
    }

    // Merge fully applied — remove journal and staging.
    let _ = std::fs::remove_file(&journal_path);
    let _ = std::fs::remove_dir_all(&staging_dir);

    if touched.is_empty() {
        tracing::warn!(
            isolated_root = %isolated.root.display(),
            "Merge completed but no files were touched — the subtask may not have written any code"
        );
    } else {
        tracing::info!(
            merged_files = touched.len(),
            files = ?touched,
            "Merge completed successfully"
        );
    }

    Ok(touched)
}

// ── Three-way merge ────────────────────────────────────────────────────────

/// Read the base (pre-fork) content of a file from the frozen base directory.
fn read_base_content(base_dir: &Path, relative: &Path) -> String {
    std::fs::read_to_string(base_dir.join(relative)).unwrap_or_default()
}

/// Build a human-readable summary showing what each side changed so Claude
/// can restructure its code to avoid the conflicting regions.
fn summarize_conflict(base: &str, main: &str, ours: &str) -> String {
    const MAX_DIFF_LINES: usize = 40;
    let mut out = String::new();

    let main_changed_lines = count_diff_lines(base, main);
    let ours_changed_lines = count_diff_lines(base, ours);
    out.push_str(&format!(
        "Main workspace changed ~{main_changed_lines} lines, your subtask changed ~{ours_changed_lines} lines.\n"
    ));

    // Show the lines that main changed (the side Claude cannot control).
    out.push_str("Lines changed in main workspace (by another subtask):\n");
    let main_diffs = abbreviated_diff(base, main, MAX_DIFF_LINES);
    if main_diffs.is_empty() {
        out.push_str("  (file is new in main)\n");
    } else {
        out.push_str(&main_diffs);
    }
    out
}

fn count_diff_lines(a: &str, b: &str) -> usize {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();
    a_lines
        .iter()
        .zip(b_lines.iter())
        .filter(|(la, lb)| la != lb)
        .count()
        + a_lines.len().abs_diff(b_lines.len())
}

fn abbreviated_diff(base: &str, changed: &str, max_lines: usize) -> String {
    let base_lines: Vec<&str> = base.lines().collect();
    let changed_lines: Vec<&str> = changed.lines().collect();
    let mut out = String::new();
    let mut shown = 0;

    let len = base_lines.len().max(changed_lines.len());
    for i in 0..len {
        let b = base_lines.get(i).copied().unwrap_or("");
        let c = changed_lines.get(i).copied().unwrap_or("");
        if b != c {
            if shown < max_lines {
                out.push_str(&format!("  L{}: -{}\n  L{}: +{}\n", i + 1, b, i + 1, c));
                shown += 2;
            } else {
                out.push_str("  ... (truncated)\n");
                break;
            }
        }
    }
    out
}

/// Line-level three-way merge.
///
/// Given three versions of a file:
/// - `base`: the common ancestor (content at fork time)
/// - `main`: the current version in the main workspace (may have been changed by other subtasks)
/// - `ours`: the version in the isolated workspace (our subtask's edits)
///
/// Returns `Ok(merged)` if changes don't overlap, `Err(())` if there's a true line-level conflict.
pub(crate) fn three_way_merge(base: &str, main: &str, ours: &str) -> Result<String, ()> {
    // If main wasn't changed from base, just take ours entirely.
    if main == base {
        return Ok(ours.to_string());
    }
    // If ours wasn't changed from base, just take main.
    if ours == base {
        return Ok(main.to_string());
    }
    // If both sides made identical changes (e.g., two subtasks independently
    // created the same new file), no conflict — just take either.
    if main == ours {
        return Ok(main.to_string());
    }
    // Both sides changed — do line-level diff3.

    let base_lines: Vec<&str> = base.lines().collect();
    let main_lines: Vec<&str> = main.lines().collect();
    let ours_lines: Vec<&str> = ours.lines().collect();

    // Compute which lines each side changed relative to base.
    let main_hunks = diff_hunks(&base_lines, &main_lines);
    let ours_hunks = diff_hunks(&base_lines, &ours_lines);

    // Check for overlapping hunks — that's a true conflict.
    if hunks_overlap(&main_hunks, &ours_hunks) {
        return Err(());
    }

    // No overlap — apply both sets of changes to base.
    let merged = apply_non_overlapping_hunks(&base_lines, &main_hunks, &ours_hunks);

    // Preserve trailing newline if any source had it.
    let needs_trailing_newline = ours.ends_with('\n') || main.ends_with('\n');
    let mut result = merged.join("\n");
    if needs_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    Ok(result)
}

// ── Diff hunks ─────────────────────────────────────────────────────────────

/// A hunk represents a contiguous range of changed lines in the base.
#[derive(Debug, Clone)]
struct DiffHunk {
    /// Start line in base (inclusive).
    base_start: usize,
    /// End line in base (exclusive).
    base_end: usize,
    /// Replacement lines from the changed version.
    new_lines: Vec<String>,
}

/// Compute hunks of changes between base and changed using the `similar` crate.
fn diff_hunks(base: &[&str], changed: &[&str]) -> Vec<DiffHunk> {
    use similar::{ChangeTag, TextDiff};

    // Append trailing newline so `TextDiff::from_lines` tokenizes every
    // line uniformly (each token ends with '\n').  Without this, the last
    // line of the shorter side would lack '\n' while the corresponding line
    // in the longer side would have it, producing a spurious diff.
    let mut base_text = base.join("\n");
    if !base_text.is_empty() {
        base_text.push('\n');
    }
    let mut changed_text = changed.join("\n");
    if !changed_text.is_empty() {
        changed_text.push('\n');
    }
    let diff = TextDiff::from_lines(&base_text, &changed_text);

    let mut hunks = Vec::new();
    let mut base_pos = 0;
    let mut current_hunk: Option<DiffHunk> = None;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }
                base_pos += 1;
            }
            ChangeTag::Delete => {
                let hunk = current_hunk.get_or_insert_with(|| DiffHunk {
                    base_start: base_pos,
                    base_end: base_pos,
                    new_lines: Vec::new(),
                });
                hunk.base_end = base_pos + 1;
                base_pos += 1;
            }
            ChangeTag::Insert => {
                let hunk = current_hunk.get_or_insert_with(|| DiffHunk {
                    base_start: base_pos,
                    base_end: base_pos,
                    new_lines: Vec::new(),
                });
                hunk.new_lines
                    .push(change.value().trim_end_matches('\n').to_string());
            }
        }
    }
    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }
    hunks
}

/// Check whether any hunks from two sets overlap in the base line space.
fn hunks_overlap(a_hunks: &[DiffHunk], b_hunks: &[DiffHunk]) -> bool {
    for a in a_hunks {
        for b in b_hunks {
            let (a_s, a_e) = (a.base_start, a.base_end.max(a.base_start + 1));
            let (b_s, b_e) = (b.base_start, b.base_end.max(b.base_start + 1));
            if a_s < b_e && b_s < a_e {
                return true;
            }
        }
    }
    false
}

/// Apply two non-overlapping sets of hunks to the base, producing merged output.
fn apply_non_overlapping_hunks(
    base: &[&str],
    main_hunks: &[DiffHunk],
    ours_hunks: &[DiffHunk],
) -> Vec<String> {
    let mut all_hunks: Vec<&DiffHunk> = main_hunks.iter().chain(ours_hunks.iter()).collect();
    all_hunks.sort_by_key(|h| (h.base_start, h.base_end));

    let mut result = Vec::new();
    let mut base_pos = 0;

    for hunk in &all_hunks {
        while base_pos < hunk.base_start && base_pos < base.len() {
            result.push(base[base_pos].to_string());
            base_pos += 1;
        }
        result.extend(hunk.new_lines.iter().cloned());
        // Guard against base_pos going backwards (e.g., a pure-insertion hunk
        // with base_end == base_start that is less than current base_pos).
        // Going backwards would re-emit lines and corrupt the merged output.
        if hunk.base_end > base_pos {
            base_pos = hunk.base_end;
        }
    }

    while base_pos < base.len() {
        result.push(base[base_pos].to_string());
        base_pos += 1;
    }

    result
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_way_merge_no_conflict_different_regions() {
        let base = "line1\nline2\nline3\nline4\nline5\n";
        let main = "line1\nMAIN_EDIT\nline3\nline4\nline5\n";
        let ours = "line1\nline2\nline3\nline4\nOURS_EDIT\n";
        let result = three_way_merge(base, main, ours).expect("should merge cleanly");
        assert!(result.contains("MAIN_EDIT"), "main edit preserved");
        assert!(result.contains("OURS_EDIT"), "ours edit preserved");
        assert!(!result.contains("line2"), "base line2 replaced by main");
        assert!(!result.contains("line5"), "base line5 replaced by ours");
    }

    #[test]
    fn three_way_merge_conflict_same_line() {
        let base = "line1\nline2\nline3\n";
        let main = "line1\nMAIN\nline3\n";
        let ours = "line1\nOURS\nline3\n";
        assert!(three_way_merge(base, main, ours).is_err());
    }

    #[test]
    fn three_way_merge_only_main_changed() {
        let base = "aaa\nbbb\n";
        let main = "aaa\nccc\n";
        let ours = "aaa\nbbb\n";
        let result = three_way_merge(base, main, ours).unwrap();
        assert_eq!(result, main);
    }

    #[test]
    fn three_way_merge_only_ours_changed() {
        let base = "aaa\nbbb\n";
        let main = "aaa\nbbb\n";
        let ours = "aaa\nxxx\n";
        let result = three_way_merge(base, main, ours).unwrap();
        assert_eq!(result, ours);
    }

    #[test]
    fn three_way_merge_both_add_at_different_positions() {
        let base = "line1\nline2\nline3\n";
        let main = "line0\nline1\nline2\nline3\n";
        let ours = "line1\nline2\nline3\nline4\n";
        let result = three_way_merge(base, main, ours).expect("should merge cleanly");
        assert!(result.contains("line0"));
        assert!(result.contains("line4"));
    }

    #[test]
    fn diff_hunks_detects_changes() {
        let base = vec!["a", "b", "c"];
        let changed = vec!["a", "x", "c"];
        let hunks = diff_hunks(&base, &changed);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].base_start, 1);
        assert_eq!(hunks[0].base_end, 2);
        assert_eq!(hunks[0].new_lines, vec!["x"]);
    }

    #[test]
    fn hunks_overlap_detects_collision() {
        let a = vec![DiffHunk {
            base_start: 2,
            base_end: 4,
            new_lines: vec![],
        }];
        let b = vec![DiffHunk {
            base_start: 3,
            base_end: 5,
            new_lines: vec![],
        }];
        assert!(hunks_overlap(&a, &b));
    }

    #[test]
    fn hunks_overlap_allows_adjacent() {
        let a = vec![DiffHunk {
            base_start: 0,
            base_end: 2,
            new_lines: vec![],
        }];
        let b = vec![DiffHunk {
            base_start: 3,
            base_end: 5,
            new_lines: vec![],
        }];
        assert!(!hunks_overlap(&a, &b));
    }

    #[test]
    fn recover_pending_merge_completes_staged_files() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // Create a file that should already exist in main.
        std::fs::write(workspace.join("existing.txt"), "original").unwrap();

        // Simulate a crash mid-merge: staging dir + journal exist but
        // files haven't been applied to the workspace yet.
        let subtask_dir = workspace.join(SCRATCH_ROOT_DIR).join("F1");
        let staging_dir = subtask_dir.join("merge-staging");
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(staging_dir.join("new_file.txt"), "merged content").unwrap();
        std::fs::create_dir_all(staging_dir.join("src")).unwrap();
        std::fs::write(staging_dir.join("src/app.rs"), "fn main() {}").unwrap();

        let journal = MergeJournal {
            subtask_id: "F1".to_string(),
            copy_files: vec!["new_file.txt".to_string(), "src/app.rs".to_string()],
            delete_files: vec![],
            staging_dir: staging_dir.to_string_lossy().into_owned(),
        };
        let journal_json = serde_json::to_string_pretty(&journal).unwrap();
        std::fs::write(subtask_dir.join(MERGE_JOURNAL), &journal_json).unwrap();

        // Run recovery.
        let recovered = recover_pending_merges(workspace.to_str().unwrap());
        assert_eq!(recovered, vec!["F1"]);

        // Verify files were applied.
        assert_eq!(
            std::fs::read_to_string(workspace.join("new_file.txt")).unwrap(),
            "merged content"
        );
        assert_eq!(
            std::fs::read_to_string(workspace.join("src/app.rs")).unwrap(),
            "fn main() {}"
        );
        // Journal should be cleaned up.
        assert!(!subtask_dir.join(MERGE_JOURNAL).exists());
        // Staging should be cleaned up.
        assert!(!staging_dir.exists());
        // Pre-existing file should be untouched.
        assert_eq!(
            std::fs::read_to_string(workspace.join("existing.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn recover_pending_merge_handles_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("to_delete.txt"), "old").unwrap();

        let subtask_dir = workspace.join(SCRATCH_ROOT_DIR).join("F2");
        let staging_dir = subtask_dir.join("merge-staging");
        std::fs::create_dir_all(&staging_dir).unwrap();

        let journal = MergeJournal {
            subtask_id: "F2".to_string(),
            copy_files: vec![],
            delete_files: vec!["to_delete.txt".to_string()],
            staging_dir: staging_dir.to_string_lossy().into_owned(),
        };
        std::fs::write(
            subtask_dir.join(MERGE_JOURNAL),
            serde_json::to_string(&journal).unwrap(),
        )
        .unwrap();

        let recovered = recover_pending_merges(workspace.to_str().unwrap());
        assert_eq!(recovered, vec!["F2"]);
        assert!(!workspace.join("to_delete.txt").exists());
    }

    #[test]
    fn no_recovery_when_no_journals() {
        let dir = tempfile::tempdir().unwrap();
        let recovered = recover_pending_merges(dir.path().to_str().unwrap());
        assert!(recovered.is_empty());
    }
}
