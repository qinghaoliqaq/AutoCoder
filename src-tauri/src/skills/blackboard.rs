/// Shared blackboard — the central coordination state for parallel subtask execution.
///
/// Heavy-lifting is delegated to sibling modules:
///   blackboard_parser — plan parsing (PLAN.md + PLAN_GRAPH.json → SubtaskCard)
///   blackboard_render — markdown rendering and label helpers
use super::blackboard_parser::build_initial_subtasks;
use super::planning_schema::SuggestedSkill;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const BLACKBOARD_JSON: &str = ".ai-dev-hub/BLACKBOARD.json";
pub(crate) const BLACKBOARD_MD: &str = ".ai-dev-hub/BLACKBOARD.md";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoardState {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubtaskState {
    Pending,
    InProgress,
    NeedsFix,
    Done,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubtaskKind {
    Feature,
    Screen,
    Task,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubtaskCard {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: SubtaskKind,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub can_run_in_parallel: bool,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub suggested_skill: Option<SuggestedSkill>,
    #[serde(default)]
    pub expected_touch: Vec<String>,
    pub status: SubtaskState,
    pub attempts: u32,
    pub latest_implementation: Option<String>,
    pub latest_review: Option<String>,
    pub review_findings: Vec<String>,
    pub files_touched: Vec<String>,
    pub isolated_workspace: Option<String>,
    pub merge_conflict: Option<String>,
    /// Implementation summaries from prior failed attempts, so Claude avoids repeating approaches.
    #[serde(default)]
    pub attempted_fixes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blackboard {
    pub task: String,
    pub state: BoardState,
    pub active_subtask_id: Option<String>,
    #[serde(default)]
    pub active_subtask_ids: Vec<String>,
    pub subtasks: Vec<SubtaskCard>,
    pub updated_at: String,
}

impl Blackboard {
    pub(crate) fn load_or_create(workspace: &str, task: &str) -> Result<Self, String> {
        let json_path = Path::new(workspace).join(BLACKBOARD_JSON);
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)
                .map_err(|e| format!("Cannot read {}: {e}", json_path.display()))?;
            let mut board = serde_json::from_str::<Blackboard>(&content)
                .map_err(|e| format!("Cannot parse {}: {e}", json_path.display()))?;
            if board.reset_transient_runtime_state() {
                board.persist(workspace)?;
            }
            return Ok(board);
        }

        let plan_path = Path::new(workspace).join(".ai-dev-hub/PLAN.md");
        let plan = std::fs::read_to_string(&plan_path)
            .map_err(|e| format!("Cannot read {}: {e}", plan_path.display()))?;
        let subtasks = build_initial_subtasks(workspace, &plan);
        if subtasks.is_empty() {
            return Err(
                "PLAN.md does not contain any checklist subtasks for code mode".to_string(),
            );
        }

        let board = Blackboard {
            task: task.to_string(),
            state: BoardState::Pending,
            active_subtask_id: None,
            active_subtask_ids: Vec::new(),
            subtasks,
            updated_at: now_string(),
        };
        board.persist(workspace)?;
        Ok(board)
    }

    pub(crate) fn persist(&self, workspace: &str) -> Result<(), String> {
        let ws = Path::new(workspace);
        let json_path = ws.join(BLACKBOARD_JSON);
        let md_path = ws.join(BLACKBOARD_MD);
        if let Some(parent) = json_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Cannot serialize blackboard: {e}"))?;
        std::fs::write(&json_path, json)
            .map_err(|e| format!("Cannot write {}: {e}", json_path.display()))?;
        std::fs::write(&md_path, self.render_markdown())
            .map_err(|e| format!("Cannot write {}: {e}", md_path.display()))?;
        Ok(())
    }

    pub(crate) fn schedulable_subtasks(&self) -> Vec<SubtaskCard> {
        self.subtasks
            .iter()
            .filter(|card| matches!(card.status, SubtaskState::Pending | SubtaskState::NeedsFix))
            .filter(|card| {
                !self
                    .active_subtask_ids
                    .iter()
                    .any(|active| active == &card.id)
            })
            .filter(|card| {
                card.depends_on.iter().all(|dependency| {
                    self.subtasks.iter().any(|candidate| {
                        candidate.id == *dependency
                            && matches!(candidate.status, SubtaskState::Done)
                    })
                })
            })
            .cloned()
            .collect()
    }

    fn reset_transient_runtime_state(&mut self) -> bool {
        let before = self.clone();
        self.active_subtask_id = None;
        self.active_subtask_ids.clear();

        for card in &mut self.subtasks {
            card.isolated_workspace = None;
            if matches!(card.status, SubtaskState::InProgress) {
                card.status = if card.review_findings.is_empty() {
                    SubtaskState::Pending
                } else {
                    SubtaskState::NeedsFix
                };
            }
            // Reset Failed subtasks so they can be retried when the user
            // restarts the code skill.  Clear stale review findings because
            // the isolated workspace is gone — keeping them would cause
            // build_fix_prompt to tell Claude to "fix existing code" in a
            // fresh empty workspace.  The attempted_fixes history is kept so
            // Claude knows what approaches already failed.
            if matches!(card.status, SubtaskState::Failed) {
                card.attempts = 0;
                card.review_findings.clear();
                card.merge_conflict = None;
                card.status = SubtaskState::Pending;
            }
        }

        self.state = if self
            .subtasks
            .iter()
            .all(|card| matches!(card.status, SubtaskState::Done))
        {
            BoardState::Completed
        } else {
            BoardState::Pending
        };
        let changed = *self != before;
        if changed {
            self.updated_at = now_string();
        }
        changed
    }

    pub(crate) fn begin_attempt(&mut self, subtask_id: &str) -> Result<u32, String> {
        self.state = BoardState::InProgress;
        self.set_active_subtask(subtask_id);
        self.updated_at = now_string();
        let card = self.subtask_mut(subtask_id)?;
        card.attempts += 1;
        card.status = SubtaskState::InProgress;
        // Note: merge_conflict is intentionally NOT cleared here.
        // It is cleared on success in record_review(passed=true), so that
        // build_fix_prompt can still see it during the retry attempt.
        Ok(card.attempts)
    }

    pub(crate) fn record_implementation(
        &mut self,
        subtask_id: &str,
        summary: String,
        files_touched: Vec<String>,
    ) -> Result<(), String> {
        let card = self.subtask_mut(subtask_id)?;
        card.latest_implementation = Some(summary);
        card.files_touched = files_touched;
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn set_isolated_workspace(
        &mut self,
        subtask_id: &str,
        path: Option<String>,
    ) -> Result<(), String> {
        let card = self.subtask_mut(subtask_id)?;
        card.isolated_workspace = path;
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn record_review(
        &mut self,
        subtask_id: &str,
        passed: bool,
        summary: String,
        findings: Vec<String>,
    ) -> Result<(), String> {
        let card = self.subtask_mut(subtask_id)?;
        // On failure, archive the current implementation summary so the next
        // attempt knows what was already tried and can avoid repeating it.
        if !passed {
            if let Some(impl_summary) = card.latest_implementation.as_ref() {
                if !impl_summary.is_empty() {
                    card.attempted_fixes
                        .push(format!("Attempt {}: {}", card.attempts, impl_summary));
                }
            }
        }
        card.latest_review = Some(summary);
        card.review_findings = findings;
        card.status = if passed {
            SubtaskState::Done
        } else {
            SubtaskState::NeedsFix
        };
        if passed {
            card.merge_conflict = None;
        }
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn record_merge_conflict(
        &mut self,
        subtask_id: &str,
        summary: String,
        findings: Vec<String>,
        conflict: String,
    ) -> Result<(), String> {
        let card = self.subtask_mut(subtask_id)?;
        if let Some(impl_summary) = card.latest_implementation.as_ref() {
            if !impl_summary.is_empty() {
                card.attempted_fixes.push(format!(
                    "Attempt {} (merge conflict): {}",
                    card.attempts, impl_summary
                ));
            }
        }
        card.latest_review = Some(summary);
        card.review_findings = findings;
        card.merge_conflict = Some(conflict);
        card.status = SubtaskState::NeedsFix;
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn mark_failed(&mut self, subtask_id: &str, reason: String) -> Result<(), String> {
        self.state = BoardState::Failed;
        self.remove_active_subtask(subtask_id);
        let card = self.subtask_mut(subtask_id)?;
        card.status = SubtaskState::Failed;
        card.latest_review = Some(reason);
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn complete_if_finished(&mut self) {
        if self
            .subtasks
            .iter()
            .all(|card| matches!(card.status, SubtaskState::Done))
        {
            self.state = BoardState::Completed;
            self.active_subtask_id = None;
            self.active_subtask_ids.clear();
            self.updated_at = now_string();
        }
    }

    pub(crate) fn finish_active_subtask(&mut self, subtask_id: &str) {
        self.remove_active_subtask(subtask_id);
        self.updated_at = now_string();
    }

    pub(crate) fn subtask(&self, subtask_id: &str) -> Result<&SubtaskCard, String> {
        self.subtasks
            .iter()
            .find(|card| card.id == subtask_id)
            .ok_or_else(|| format!("Unknown subtask: {subtask_id}"))
    }

    fn subtask_mut(&mut self, subtask_id: &str) -> Result<&mut SubtaskCard, String> {
        self.subtasks
            .iter_mut()
            .find(|card| card.id == subtask_id)
            .ok_or_else(|| format!("Unknown subtask: {subtask_id}"))
    }

    fn set_active_subtask(&mut self, subtask_id: &str) {
        let subtask_id = subtask_id.to_string();
        if !self.active_subtask_ids.iter().any(|id| id == &subtask_id) {
            self.active_subtask_ids.push(subtask_id.clone());
            self.active_subtask_ids.sort();
        }
        self.active_subtask_id = self.active_subtask_ids.first().cloned();
    }

    fn remove_active_subtask(&mut self, subtask_id: &str) {
        self.active_subtask_ids.retain(|id| id != subtask_id);
        self.active_subtask_id = self.active_subtask_ids.first().cloned();
    }
}

pub(crate) fn sanitize_persisted_state(workspace: &str) -> Result<(), String> {
    let json_path = Path::new(workspace).join(BLACKBOARD_JSON);
    if !json_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("Cannot read {}: {e}", json_path.display()))?;
    let mut board = serde_json::from_str::<Blackboard>(&content)
        .map_err(|e| format!("Cannot parse {}: {e}", json_path.display()))?;

    if board.reset_transient_runtime_state() {
        board.persist(workspace)?;
    }

    Ok(())
}

pub(crate) fn tick_plan_checkbox(workspace: &str, subtask_id: &str) -> Result<(), String> {
    let plan_path = Path::new(workspace).join(".ai-dev-hub/PLAN.md");
    let content = std::fs::read_to_string(&plan_path)
        .map_err(|e| format!("Cannot read {}: {e}", plan_path.display()))?;
    let target = format!("**{subtask_id}.");
    let mut changed = false;
    let mut lines = Vec::new();

    for line in content.lines() {
        if !changed && line.contains(&target) && line.trim_start().starts_with("- [") {
            lines.push(line.replacen("- [ ]", "- [x]", 1));
            changed = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if changed {
        std::fs::write(&plan_path, format!("{}\n", lines.join("\n")))
            .map_err(|e| format!("Cannot write {}: {e}", plan_path.display()))?;
    }
    Ok(())
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_plan_checkbox_marks_matching_item() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        let plan_path = dir.path().join(".ai-dev-hub/PLAN.md");
        std::fs::write(
            &plan_path,
            "- [ ] **F1. User login** - POST /login\n- [ ] **P1. Login screen** - form\n",
        )
        .unwrap();

        tick_plan_checkbox(dir.path().to_str().unwrap(), "P1").unwrap();
        let updated = std::fs::read_to_string(plan_path).unwrap();
        assert!(updated.contains("- [x] **P1. Login screen**"));
        assert!(updated.contains("- [ ] **F1. User login**"));
    }

    #[test]
    fn sanitize_persisted_state_rewrites_transient_runtime_fields() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::InProgress,
            active_subtask_id: Some("F1".to_string()),
            active_subtask_ids: vec!["F1".to_string(), "F2".to_string()],
            subtasks: vec![
                SubtaskCard {
                    id: "F1".to_string(),
                    title: "One".to_string(),
                    description: "desc".to_string(),
                    kind: SubtaskKind::Feature,
                    depends_on: Vec::new(),
                    can_run_in_parallel: true,
                    parallel_group: None,
                    suggested_skill: None,
                    expected_touch: Vec::new(),
                    status: SubtaskState::InProgress,
                    attempts: 1,
                    latest_implementation: None,
                    latest_review: None,
                    review_findings: Vec::new(),
                    files_touched: Vec::new(),
                    isolated_workspace: Some("/tmp/demo".to_string()),
                    merge_conflict: None,
                    attempted_fixes: Vec::new(),
                },
                SubtaskCard {
                    id: "F2".to_string(),
                    title: "Two".to_string(),
                    description: "desc".to_string(),
                    kind: SubtaskKind::Feature,
                    depends_on: Vec::new(),
                    can_run_in_parallel: true,
                    parallel_group: None,
                    suggested_skill: None,
                    expected_touch: Vec::new(),
                    status: SubtaskState::InProgress,
                    attempts: 2,
                    latest_implementation: None,
                    latest_review: None,
                    review_findings: vec!["fix me".to_string()],
                    files_touched: Vec::new(),
                    isolated_workspace: Some("/tmp/demo-2".to_string()),
                    merge_conflict: None,
                    attempted_fixes: Vec::new(),
                },
            ],
            updated_at: "before".to_string(),
        };
        board.persist(workspace).unwrap();

        sanitize_persisted_state(workspace).unwrap();

        let persisted = std::fs::read_to_string(dir.path().join(BLACKBOARD_JSON)).unwrap();
        let restored = serde_json::from_str::<Blackboard>(&persisted).unwrap();
        assert_eq!(restored.state, BoardState::Pending);
        assert_eq!(restored.active_subtask_id, None);
        assert!(restored.active_subtask_ids.is_empty());
        assert_eq!(restored.subtasks[0].status, SubtaskState::Pending);
        assert_eq!(restored.subtasks[1].status, SubtaskState::NeedsFix);
        assert_eq!(restored.subtasks[0].isolated_workspace, None);
        assert_eq!(restored.subtasks[1].isolated_workspace, None);
        assert_ne!(restored.updated_at, "before");
    }

    #[test]
    fn schedulable_subtasks_require_dependencies_done() {
        let board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::Pending,
            active_subtask_id: None,
            active_subtask_ids: Vec::new(),
            subtasks: vec![
                SubtaskCard {
                    id: "F1".to_string(),
                    title: "API".to_string(),
                    description: "desc".to_string(),
                    kind: SubtaskKind::Feature,
                    depends_on: Vec::new(),
                    can_run_in_parallel: true,
                    parallel_group: None,
                    suggested_skill: None,
                    expected_touch: Vec::new(),
                    status: SubtaskState::Done,
                    attempts: 0,
                    latest_implementation: None,
                    latest_review: None,
                    review_findings: Vec::new(),
                    files_touched: Vec::new(),
                    isolated_workspace: None,
                    merge_conflict: None,
                    attempted_fixes: Vec::new(),
                },
                SubtaskCard {
                    id: "P1".to_string(),
                    title: "Page".to_string(),
                    description: "desc".to_string(),
                    kind: SubtaskKind::Screen,
                    depends_on: vec!["F1".to_string()],
                    can_run_in_parallel: true,
                    parallel_group: None,
                    suggested_skill: None,
                    expected_touch: Vec::new(),
                    status: SubtaskState::Pending,
                    attempts: 0,
                    latest_implementation: None,
                    latest_review: None,
                    review_findings: Vec::new(),
                    files_touched: Vec::new(),
                    isolated_workspace: None,
                    merge_conflict: None,
                    attempted_fixes: Vec::new(),
                },
            ],
            updated_at: "now".to_string(),
        };

        let ready = board.schedulable_subtasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "P1");
    }

    #[test]
    fn begin_attempt_preserves_merge_conflict_for_retry_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::InProgress,
            active_subtask_id: None,
            active_subtask_ids: Vec::new(),
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "API".to_string(),
                description: "desc".to_string(),
                kind: SubtaskKind::Feature,
                depends_on: Vec::new(),
                can_run_in_parallel: true,
                parallel_group: None,
                suggested_skill: None,
                expected_touch: Vec::new(),
                status: SubtaskState::NeedsFix,
                attempts: 1,
                latest_implementation: Some("First try".to_string()),
                latest_review: None,
                review_findings: vec!["conflict".to_string()],
                files_touched: Vec::new(),
                isolated_workspace: None,
                merge_conflict: Some("Conflict in shared/db.rs".to_string()),
                attempted_fixes: Vec::new(),
            }],
            updated_at: "before".to_string(),
        };
        board.persist(workspace).unwrap();

        board.begin_attempt("F1").unwrap();

        // merge_conflict must survive so build_fix_prompt can render it.
        assert_eq!(
            board.subtask("F1").unwrap().merge_conflict.as_deref(),
            Some("Conflict in shared/db.rs")
        );
    }

    #[test]
    fn record_review_archives_attempted_fix_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::InProgress,
            active_subtask_id: Some("F1".to_string()),
            active_subtask_ids: vec!["F1".to_string()],
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "API".to_string(),
                description: "desc".to_string(),
                kind: SubtaskKind::Feature,
                depends_on: Vec::new(),
                can_run_in_parallel: true,
                parallel_group: None,
                suggested_skill: None,
                expected_touch: Vec::new(),
                status: SubtaskState::InProgress,
                attempts: 1,
                latest_implementation: Some("Added CRUD endpoints".to_string()),
                latest_review: None,
                review_findings: Vec::new(),
                files_touched: Vec::new(),
                isolated_workspace: None,
                merge_conflict: None,
                attempted_fixes: Vec::new(),
            }],
            updated_at: "before".to_string(),
        };
        board.persist(workspace).unwrap();

        board
            .record_review(
                "F1",
                false,
                "Missing validation".to_string(),
                vec!["fix it".to_string()],
            )
            .unwrap();

        let card = board.subtask("F1").unwrap();
        assert_eq!(card.attempted_fixes.len(), 1);
        assert!(card.attempted_fixes[0].contains("Added CRUD endpoints"));
        assert!(card.attempted_fixes[0].starts_with("Attempt 1:"));
    }

    #[test]
    fn record_review_success_clears_merge_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::InProgress,
            active_subtask_id: Some("F1".to_string()),
            active_subtask_ids: vec!["F1".to_string()],
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "API".to_string(),
                description: "desc".to_string(),
                kind: SubtaskKind::Feature,
                depends_on: Vec::new(),
                can_run_in_parallel: true,
                parallel_group: None,
                suggested_skill: None,
                expected_touch: Vec::new(),
                status: SubtaskState::InProgress,
                attempts: 2,
                latest_implementation: Some("Fixed it".to_string()),
                latest_review: None,
                review_findings: Vec::new(),
                files_touched: Vec::new(),
                isolated_workspace: None,
                merge_conflict: Some("old conflict".to_string()),
                attempted_fixes: vec!["Attempt 1: first try".to_string()],
            }],
            updated_at: "before".to_string(),
        };
        board.persist(workspace).unwrap();

        board
            .record_review("F1", true, "Looks good".to_string(), Vec::new())
            .unwrap();

        let card = board.subtask("F1").unwrap();
        assert_eq!(card.merge_conflict, None);
        // attempted_fixes should NOT grow on success.
        assert_eq!(card.attempted_fixes.len(), 1);
    }
}
