use super::{blackboard::SubtaskCard, planning_schema::SubtaskAcceptance};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const VERIFIER_RESULT_JSON: &str = "verifier-result.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifierResult {
    pub version: u32,
    pub subtask_id: String,
    pub attempt: u32,
    pub passed: bool,
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub observed_files: Vec<String>,
    #[serde(default)]
    pub expected_touch: Vec<String>,
    #[serde(default)]
    pub sensitive_touches: Vec<String>,
}

pub(crate) fn run_and_persist(
    workspace: &str,
    isolated_workspace: &Path,
    card: &SubtaskCard,
    acceptance: Option<&SubtaskAcceptance>,
    observed_files: &[String],
    implementation_summary: &str,
) -> Result<VerifierResult, String> {
    let result = verify(card, acceptance, observed_files, implementation_summary);
    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| format!("Cannot serialize {VERIFIER_RESULT_JSON}: {e}"))?;

    let isolated_path = isolated_workspace.join(VERIFIER_RESULT_JSON);
    let tmp_path = isolated_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("Cannot write {}: {e}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &isolated_path)
        .map_err(|e| format!("Cannot rename to {}: {e}", isolated_path.display()))?;

    // Archive write is supplementary (for debugging/evidence) — must not
    // kill the subtask if the archive directory is inaccessible.
    let archive_path = archive_path(workspace, &card.id, card.attempts);
    if let Some(parent) = archive_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&archive_path, json) {
        tracing::warn!("Failed to archive verifier result (non-fatal): {e}");
    }

    Ok(result)
}

pub(crate) fn archive_relative_path(subtask_id: &str, attempt: u32) -> String {
    format!(".ai-dev-hub/verifier/{subtask_id}/attempt-{attempt}.json")
}

fn archive_path(workspace: &str, subtask_id: &str, attempt: u32) -> std::path::PathBuf {
    Path::new(workspace).join(archive_relative_path(subtask_id, attempt))
}

fn verify(
    card: &SubtaskCard,
    acceptance: Option<&SubtaskAcceptance>,
    observed_files: &[String],
    implementation_summary: &str,
) -> VerifierResult {
    let mut findings = Vec::new();
    let mut warnings = Vec::new();
    let observed_files = dedupe(observed_files);
    let sensitive_touches = observed_files
        .iter()
        .filter(|path| is_sensitive_path(path))
        .cloned()
        .collect::<Vec<_>>();

    if observed_files.is_empty() {
        findings.push("No file changes were detected for this subtask attempt.".to_string());
    }

    if !card.expected_touch.is_empty() {
        let unexpected = observed_files
            .iter()
            .filter(|path| !matches_expected_touch(path, &card.expected_touch))
            .cloned()
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            warnings.push(format!(
                "Touched files outside expected scope: {}. Expected touch: {}. Codex should confirm the shared-code integration was necessary.",
                unexpected.join(", "),
                card.expected_touch.join(", ")
            ));
        }
    }

    let sensitive_without_plan = sensitive_touches
        .iter()
        .filter(|path| !matches_expected_touch(path, &card.expected_touch))
        .cloned()
        .collect::<Vec<_>>();
    if !sensitive_without_plan.is_empty() {
        // Warn instead of block.  Subtasks often need to touch package.json
        // or Cargo.toml (e.g. adding dependencies) even if expected_touch
        // doesn't list them.  Blocking here wastes all retry attempts on an
        // issue Claude cannot fix.  Codex review will catch truly wrong edits.
        warnings.push(format!(
            "Sensitive paths were changed without explicit plan coverage: {}. Codex should verify these changes are necessary.",
            sensitive_without_plan.join(", ")
        ));
    }

    if let Some(acceptance) = acceptance {
        let requires_tests = acceptance
            .evidence_required
            .iter()
            .any(|item| item.to_ascii_lowercase().contains("test"));
        let touched_test_file = observed_files.iter().any(|path| is_test_path(path));
        if requires_tests && !touched_test_file {
            warnings.push(
                "Acceptance expects test evidence, but this attempt did not touch any obvious test file."
                    .to_string(),
            );
        }

        if !acceptance.must_have.is_empty() && implementation_summary.trim().len() < 20 {
            warnings.push(
                "Implementation summary is too thin to confirm the declared must_have criteria."
                    .to_string(),
            );
        }
    }

    let passed = findings.is_empty();
    let summary = if passed {
        if warnings.is_empty() {
            "Verifier passed with no blocking findings.".to_string()
        } else {
            format!(
                "Verifier passed with {} warning(s) that Codex review should double-check.",
                warnings.len()
            )
        }
    } else {
        format!(
            "Verifier blocked this subtask with {} finding(s).",
            findings.len()
        )
    };

    VerifierResult {
        version: 1,
        subtask_id: card.id.clone(),
        attempt: card.attempts,
        passed,
        summary,
        findings,
        warnings,
        observed_files,
        expected_touch: card.expected_touch.clone(),
        sensitive_touches,
    }
}

fn dedupe(items: &[String]) -> Vec<String> {
    let mut items = items.to_vec();
    items.sort();
    items.dedup();
    items
}

fn matches_expected_touch(path: &str, expected_touch: &[String]) -> bool {
    if expected_touch.is_empty() {
        return true;
    }

    expected_touch.iter().any(|expected| {
        let expected = expected.trim_matches('/');
        path == expected || path.starts_with(&format!("{expected}/"))
    })
}

fn is_sensitive_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered == "cargo.toml"
        || lowered == "package.json"
        || lowered == "pnpm-lock.yaml"
        || lowered == "package-lock.json"
        || lowered == "yarn.lock"
        || lowered == "dockerfile"
        || lowered.starts_with(".github/")
        || lowered.starts_with("infra/")
        || lowered.starts_with("deploy/")
        || lowered == "src-tauri/src/lib.rs"
        || lowered == "src-tauri/tauri.conf.json"
}

fn is_test_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    lowered.contains("/test")
        || lowered.contains("/tests")
        || lowered.contains(".test.")
        || lowered.contains(".spec.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::blackboard::{SubtaskKind, SubtaskState};

    fn sample_card() -> SubtaskCard {
        SubtaskCard {
            id: "F1".to_string(),
            title: "Jobs API".to_string(),
            description: "Build jobs".to_string(),
            kind: SubtaskKind::Feature,
            depends_on: Vec::new(),
            can_run_in_parallel: true,
            parallel_group: None,
            suggested_skill: None,
            expected_touch: vec!["src/jobs".to_string()],
            status: SubtaskState::InProgress,
            attempts: 1,
            latest_implementation: Some("Implemented CRUD".to_string()),
            latest_review: None,
            review_findings: Vec::new(),
            files_touched: vec!["src/jobs/api.rs".to_string()],
            isolated_workspace: None,
            merge_conflict: None,
            attempted_fixes: Vec::new(),
        }
    }

    #[test]
    fn verifier_warns_on_unexpected_non_sensitive_touch() {
        let card = sample_card();
        let result = verify(
            &card,
            None,
            &["src/jobs/api.rs".to_string(), "src/auth/mod.rs".to_string()],
            "Implemented CRUD",
        );
        assert!(result.passed);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("outside expected scope")));
    }

    #[test]
    fn verifier_warns_on_sensitive_touch_without_plan() {
        let card = sample_card();
        let result = verify(&card, None, &["Cargo.toml".to_string()], "Updated deps");
        // Sensitive touches without plan coverage are now warnings, not
        // blocking findings — Codex review decides if they are justified.
        assert!(result.passed);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Sensitive paths")));
    }

    #[test]
    fn verifier_warns_when_test_evidence_is_expected_but_missing() {
        let card = sample_card();
        let acceptance = SubtaskAcceptance {
            subtask_id: "F1".to_string(),
            must_have: vec!["CRUD".to_string()],
            must_not: Vec::new(),
            evidence_required: vec!["API tests".to_string()],
            qa_focus: Vec::new(),
        };
        let result = verify(
            &card,
            Some(&acceptance),
            &["src/jobs/api.rs".to_string()],
            "Implemented CRUD",
        );
        assert!(result.passed);
        assert!(result.warnings.iter().any(|w| w.contains("test evidence")));
    }
}
