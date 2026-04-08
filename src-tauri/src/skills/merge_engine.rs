/// Line-level three-way merge engine for parallel subtask workspaces.
///
/// When multiple subtasks modify files in parallel, the merge engine detects
/// conflicts and performs automatic three-way merges where possible.
use super::isolated_workspace::{snapshot_workspace, workspace_changes, IsolatedWorkspace};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ── Main merge entry point ─────────────────────────────────────────────────

pub(crate) fn merge_isolated_workspace(
    workspace: &str,
    isolated: &IsolatedWorkspace,
) -> Result<Vec<String>, String> {
    let main_root = Path::new(workspace);
    let isolated_after = snapshot_workspace(&isolated.root);
    let changes = workspace_changes(&isolated.base_snapshot, &isolated_after);
    let main_before = snapshot_workspace(main_root);

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
        let main_content = std::fs::read_to_string(main_root.join(path)).unwrap_or_default();
        let ours_content = std::fs::read_to_string(isolated.root.join(path)).unwrap_or_default();

        match three_way_merge(&base_content, &main_content, &ours_content) {
            Ok(merged) => merge_results.push((path.clone(), merged)),
            Err(_) => conflicts.push(path.to_string_lossy().into_owned()),
        }
    }

    if !conflicts.is_empty() {
        conflicts.sort();
        conflicts.dedup();
        return Err(format!(
            "Merge conflict on files already changed in the main workspace: {}",
            conflicts.join(", ")
        ));
    }

    // Apply clean merges.
    for (path, merged_content) in &merge_results {
        let target = main_root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, merged_content)
            .map_err(|e| format!("Cannot write merged {}: {e}", target.display()))?;
        touched.push(path.to_string_lossy().into_owned());
    }

    // Copy files that only we changed (no divergence in main).
    let merge_set: HashSet<&PathBuf> = needs_merge.iter().collect();
    for path in &changes.changed_or_created {
        if merge_set.contains(path) {
            continue; // Already handled above.
        }
        let source = isolated.root.join(path);
        let target = main_root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&source, &target).map_err(|e| {
            format!(
                "Cannot merge {} -> {}: {e}",
                source.display(),
                target.display()
            )
        })?;
        touched.push(path.to_string_lossy().into_owned());
    }

    for path in &changes.deleted {
        let target = main_root.join(path);
        if target.exists() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("Cannot remove {} during merge: {e}", target.display()))?;
            touched.push(path.to_string_lossy().into_owned());
        }
    }

    touched.sort();
    touched.dedup();
    Ok(touched)
}

// ── Three-way merge ────────────────────────────────────────────────────────

/// Read the base (pre-fork) content of a file from the frozen base directory.
fn read_base_content(base_dir: &Path, relative: &Path) -> String {
    std::fs::read_to_string(base_dir.join(relative)).unwrap_or_default()
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

    let base_text = base.join("\n");
    let changed_text = changed.join("\n");
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
        base_pos = hunk.base_end;
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
}
