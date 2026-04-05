use crate::planning_schema::{read_plan_graph, PlanSubtask, SubtaskCategory, SuggestedSkill};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub(crate) const BLACKBOARD_JSON: &str = "BLACKBOARD.json";
pub(crate) const BLACKBOARD_MD: &str = "BLACKBOARD.md";

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

        let plan_path = Path::new(workspace).join("PLAN.md");
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
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Cannot serialize blackboard: {e}"))?;
        std::fs::write(&json_path, json)
            .map_err(|e| format!("Cannot write {}: {e}", json_path.display()))?;
        std::fs::write(&md_path, self.render_markdown())
            .map_err(|e| format!("Cannot write {}: {e}", md_path.display()))?;
        Ok(())
    }

    pub(crate) fn render_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Shared Blackboard\n\n");
        out.push_str("This file is the only shared coordination state between agents.\n");
        out.push_str(
            "Agents must read this board instead of relying on direct agent-to-agent chat.\n\n",
        );
        out.push_str("## Overall Status\n\n");
        out.push_str(&format!("- Task: {}\n", self.task));
        out.push_str(&format!("- State: {}\n", board_state_label(&self.state)));
        out.push_str(&format!(
            "- Active subtask: {}\n",
            self.active_subtask_id.as_deref().unwrap_or("none")
        ));
        if !self.active_subtask_ids.is_empty() {
            out.push_str(&format!(
                "- Active subtasks: {}\n",
                self.active_subtask_ids.join(", ")
            ));
        }
        out.push_str(&format!("- Last updated: {}\n\n", self.updated_at));
        out.push_str("## Subtasks\n\n");

        for card in &self.subtasks {
            out.push_str(&format!("### {}. {}\n", card.id, card.title));
            out.push_str(&format!("- Kind: {}\n", subtask_kind_label(&card.kind)));
            out.push_str(&format!(
                "- Status: {}\n",
                subtask_state_label(&card.status)
            ));
            out.push_str(&format!("- Attempts: {}\n", card.attempts));
            out.push_str(&format!("- Description: {}\n", card.description));
            if !card.depends_on.is_empty() {
                out.push_str(&format!("- Depends on: {}\n", card.depends_on.join(", ")));
            }
            out.push_str(&format!(
                "- Parallel execution: {}\n",
                if card.can_run_in_parallel {
                    "yes"
                } else {
                    "no"
                }
            ));
            if let Some(group) = &card.parallel_group {
                out.push_str(&format!("- Parallel group: {}\n", group));
            }
            if let Some(skill) = &card.suggested_skill {
                out.push_str(&format!(
                    "- Suggested skill: {}\n",
                    suggested_skill_label(skill)
                ));
            }
            if !card.expected_touch.is_empty() {
                out.push_str("- Expected touch:\n");
                for path in &card.expected_touch {
                    out.push_str(&format!("  - {}\n", path));
                }
            }
            if let Some(summary) = &card.latest_implementation {
                out.push_str(&format!("- Latest implementation: {}\n", summary));
            }
            if let Some(summary) = &card.latest_review {
                out.push_str(&format!("- Latest review: {}\n", summary));
            }
            if !card.review_findings.is_empty() {
                out.push_str("- Review findings:\n");
                for finding in &card.review_findings {
                    out.push_str(&format!("  - {}\n", finding));
                }
            }
            if !card.files_touched.is_empty() {
                out.push_str("- Files touched:\n");
                for file in &card.files_touched {
                    out.push_str(&format!("  - {}\n", file));
                }
            }
            if let Some(path) = &card.isolated_workspace {
                out.push_str(&format!("- Isolated workspace: {}\n", path));
            }
            if let Some(conflict) = &card.merge_conflict {
                out.push_str(&format!("- Merge conflict: {}\n", conflict));
            }
            out.push('\n');
        }
        out
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
        }

        self.state = if self
            .subtasks
            .iter()
            .all(|card| matches!(card.status, SubtaskState::Done))
        {
            BoardState::Completed
        } else if matches!(self.state, BoardState::Failed) {
            BoardState::Failed
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
        card.merge_conflict = None;
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
    let plan_path = Path::new(workspace).join("PLAN.md");
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

fn parse_plan_subtasks(plan: &str) -> Vec<SubtaskCard> {
    plan.lines().filter_map(parse_checklist_line).collect()
}

fn build_initial_subtasks(workspace: &str, plan: &str) -> Vec<SubtaskCard> {
    let checked_ids = parse_checked_subtask_ids(plan);
    match read_plan_graph(workspace) {
        Ok(Some(graph)) => {
            let subtasks = graph
                .subtasks
                .iter()
                .map(|subtask| {
                    subtask_from_plan_graph(subtask, checked_ids.contains(subtask.id.as_str()))
                })
                .collect::<Vec<_>>();
            if !subtasks.is_empty() {
                return subtasks;
            }
            parse_plan_subtasks(plan)
        }
        _ => parse_plan_subtasks(plan),
    }
}

fn parse_checked_subtask_ids(plan: &str) -> HashSet<String> {
    plan.lines()
        .filter_map(parse_checklist_line)
        .filter(|card| matches!(card.status, SubtaskState::Done))
        .map(|card| card.id)
        .collect()
}

fn parse_checklist_line(line: &str) -> Option<SubtaskCard> {
    let trimmed = line.trim();
    if !trimmed.starts_with("- [") || !trimmed.contains("**") {
        return None;
    }
    let is_checked = trimmed
        .get(3..4)
        .map(|value| value.eq_ignore_ascii_case("x"))
        .unwrap_or(false);

    let (_, after_open) = trimmed.split_once("**")?;
    let (header, after_close) = after_open.split_once("**")?;
    let (id, title) = header.split_once('.')?;
    let id = id.trim().to_string();
    let title = title.trim().to_string();
    if id.is_empty() || title.is_empty() {
        return None;
    }

    let description = after_close
        .trim()
        .trim_start_matches('-')
        .trim_start_matches('—')
        .trim_start_matches(':')
        .trim()
        .to_string();

    let kind = if id.starts_with('F') {
        SubtaskKind::Feature
    } else if id.starts_with('P') {
        SubtaskKind::Screen
    } else {
        SubtaskKind::Task
    };

    Some(SubtaskCard {
        id,
        title,
        description,
        kind,
        depends_on: Vec::new(),
        can_run_in_parallel: true,
        parallel_group: None,
        suggested_skill: None,
        expected_touch: Vec::new(),
        status: if is_checked {
            SubtaskState::Done
        } else {
            SubtaskState::Pending
        },
        attempts: 0,
        latest_implementation: None,
        latest_review: if is_checked {
            Some("Recovered from checked PLAN.md entry.".to_string())
        } else {
            None
        },
        review_findings: Vec::new(),
        files_touched: Vec::new(),
        isolated_workspace: None,
        merge_conflict: None,
    })
}

fn subtask_from_plan_graph(subtask: &PlanSubtask, is_checked: bool) -> SubtaskCard {
    SubtaskCard {
        id: subtask.id.clone(),
        title: subtask.title.clone(),
        description: subtask.description.clone(),
        kind: subtask_kind_from_category(&subtask.category, &subtask.id),
        depends_on: subtask.depends_on.clone(),
        can_run_in_parallel: subtask.can_run_in_parallel,
        parallel_group: subtask.parallel_group.clone(),
        suggested_skill: subtask.suggested_skill.clone(),
        expected_touch: subtask.expected_touch.clone(),
        status: if is_checked {
            SubtaskState::Done
        } else {
            SubtaskState::Pending
        },
        attempts: 0,
        latest_implementation: None,
        latest_review: if is_checked {
            Some("Recovered from checked PLAN.md entry.".to_string())
        } else {
            None
        },
        review_findings: Vec::new(),
        files_touched: Vec::new(),
        isolated_workspace: None,
        merge_conflict: None,
    }
}

fn subtask_kind_from_category(category: &SubtaskCategory, id: &str) -> SubtaskKind {
    match category {
        SubtaskCategory::Frontend => SubtaskKind::Screen,
        SubtaskCategory::Backend => SubtaskKind::Feature,
        SubtaskCategory::Fullstack => {
            if id.starts_with('P') {
                SubtaskKind::Screen
            } else {
                SubtaskKind::Feature
            }
        }
        SubtaskCategory::Infra | SubtaskCategory::Docs => SubtaskKind::Task,
    }
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn board_state_label(state: &BoardState) -> &'static str {
    match state {
        BoardState::Pending => "pending",
        BoardState::InProgress => "in_progress",
        BoardState::Completed => "completed",
        BoardState::Failed => "failed",
    }
}

fn subtask_state_label(state: &SubtaskState) -> &'static str {
    match state {
        SubtaskState::Pending => "pending",
        SubtaskState::InProgress => "in_progress",
        SubtaskState::NeedsFix => "needs_fix",
        SubtaskState::Done => "done",
        SubtaskState::Failed => "failed",
    }
}

fn subtask_kind_label(kind: &SubtaskKind) -> &'static str {
    match kind {
        SubtaskKind::Feature => "feature",
        SubtaskKind::Screen => "screen",
        SubtaskKind::Task => "task",
    }
}

fn suggested_skill_label(skill: &SuggestedSkill) -> &'static str {
    match skill {
        SuggestedSkill::FrontendDev => "frontend-dev",
        SuggestedSkill::FullstackDev => "fullstack-dev",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_feature_and_screen_checklists() {
        let plan = "\
- [ ] **F1. User login** - POST /login with email/password\n\
- [ ] **P1. Login screen** - form with validation\n";
        let cards = parse_plan_subtasks(plan);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "F1");
        assert_eq!(cards[0].kind, SubtaskKind::Feature);
        assert_eq!(cards[1].id, "P1");
        assert_eq!(cards[1].kind, SubtaskKind::Screen);
    }

    #[test]
    fn parse_checked_items_as_done() {
        let plan = "- [x] **F1. User login** - already shipped\n";
        let cards = parse_plan_subtasks(plan);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].status, SubtaskState::Done);
        assert_eq!(
            cards[0].latest_review.as_deref(),
            Some("Recovered from checked PLAN.md entry.")
        );
    }

    #[test]
    fn tick_plan_checkbox_marks_matching_item() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("PLAN.md");
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
    fn render_markdown_mentions_board_contract() {
        let board = Blackboard {
            task: "demo".to_string(),
            state: BoardState::Pending,
            active_subtask_id: None,
            active_subtask_ids: Vec::new(),
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "Demo".to_string(),
                description: "desc".to_string(),
                kind: SubtaskKind::Feature,
                depends_on: Vec::new(),
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
            }],
            updated_at: "now".to_string(),
        };

        let markdown = board.render_markdown();
        assert!(markdown.contains("only shared coordination state"));
        assert!(markdown.contains("### F1. Demo"));
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
    fn load_or_create_prefers_plan_graph_when_available() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("PLAN.md"),
            "- [x] **F1. Jobs API** - Build job routes\n- [ ] **P1. Jobs Page** - Build jobs page\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(crate::planning_schema::PLAN_GRAPH_JSON),
            r#"{
                "version": 1,
                "project_name": "Hiring Hub",
                "project_goal": "Coordinate hiring workflows",
                "subtasks": [
                    {
                        "id": "F1",
                        "title": "Jobs API",
                        "description": "Build job routes",
                        "category": "backend",
                        "depends_on": [],
                        "parallel_group": "backend-core",
                        "can_run_in_parallel": true,
                        "suggested_skill": "fullstack-dev",
                        "expected_touch": ["backend/jobs"]
                    },
                    {
                        "id": "P1",
                        "title": "Jobs Page",
                        "description": "Build jobs page",
                        "category": "frontend",
                        "depends_on": ["F1"],
                        "parallel_group": "ui-main",
                        "can_run_in_parallel": false,
                        "suggested_skill": "frontend-dev",
                        "expected_touch": ["src/pages/jobs"]
                    }
                ]
            }"#,
        )
        .unwrap();

        let board = Blackboard::load_or_create(dir.path().to_str().unwrap(), "demo").unwrap();
        assert_eq!(board.subtasks.len(), 2);
        assert_eq!(board.subtasks[0].status, SubtaskState::Done);
        assert_eq!(board.subtasks[1].depends_on, vec!["F1".to_string()]);
        assert!(!board.subtasks[1].can_run_in_parallel);
        assert_eq!(
            board.subtasks[1].suggested_skill,
            Some(crate::planning_schema::SuggestedSkill::FrontendDev)
        );
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
                },
            ],
            updated_at: "now".to_string(),
        };

        let ready = board.schedulable_subtasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "P1");
    }
}
