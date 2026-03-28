use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    pub status: SubtaskState,
    pub attempts: u32,
    pub latest_implementation: Option<String>,
    pub latest_review: Option<String>,
    pub review_findings: Vec<String>,
    pub files_touched: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blackboard {
    pub task: String,
    pub state: BoardState,
    pub active_subtask_id: Option<String>,
    pub subtasks: Vec<SubtaskCard>,
    pub updated_at: String,
}

impl Blackboard {
    pub(crate) fn load_or_create(workspace: &str, task: &str) -> Result<Self, String> {
        let json_path = Path::new(workspace).join(BLACKBOARD_JSON);
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)
                .map_err(|e| format!("Cannot read {}: {e}", json_path.display()))?;
            let board = serde_json::from_str::<Blackboard>(&content)
                .map_err(|e| format!("Cannot parse {}: {e}", json_path.display()))?;
            return Ok(board);
        }

        let plan_path = Path::new(workspace).join("PLAN.md");
        let plan = std::fs::read_to_string(&plan_path)
            .map_err(|e| format!("Cannot read {}: {e}", plan_path.display()))?;
        let subtasks = parse_plan_subtasks(&plan);
        if subtasks.is_empty() {
            return Err("PLAN.md does not contain any checklist subtasks for code mode".to_string());
        }

        let board = Blackboard {
            task: task.to_string(),
            state: BoardState::Pending,
            active_subtask_id: None,
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
        out.push_str("Agents must read this board instead of relying on direct agent-to-agent chat.\n\n");
        out.push_str("## Overall Status\n\n");
        out.push_str(&format!("- Task: {}\n", self.task));
        out.push_str(&format!("- State: {}\n", board_state_label(&self.state)));
        out.push_str(&format!(
            "- Active subtask: {}\n",
            self.active_subtask_id.as_deref().unwrap_or("none")
        ));
        out.push_str(&format!("- Last updated: {}\n\n", self.updated_at));
        out.push_str("## Subtasks\n\n");

        for card in &self.subtasks {
            out.push_str(&format!("### {}. {}\n", card.id, card.title));
            out.push_str(&format!("- Kind: {}\n", subtask_kind_label(&card.kind)));
            out.push_str(&format!("- Status: {}\n", subtask_state_label(&card.status)));
            out.push_str(&format!("- Attempts: {}\n", card.attempts));
            out.push_str(&format!("- Description: {}\n", card.description));
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
            out.push('\n');
        }
        out
    }

    pub(crate) fn pending_subtasks(&self) -> Vec<SubtaskCard> {
        self.subtasks
            .iter()
            .filter(|card| !matches!(card.status, SubtaskState::Done))
            .cloned()
            .collect()
    }

    pub(crate) fn begin_attempt(&mut self, subtask_id: &str) -> Result<u32, String> {
        self.state = BoardState::InProgress;
        self.active_subtask_id = Some(subtask_id.to_string());
        self.updated_at = now_string();
        let card = self.subtask_mut(subtask_id)?;
        card.attempts += 1;
        card.status = if card.review_findings.is_empty() {
            SubtaskState::InProgress
        } else {
            SubtaskState::NeedsFix
        };
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
        card.status = if passed { SubtaskState::Done } else { SubtaskState::NeedsFix };
        self.updated_at = now_string();
        Ok(())
    }

    pub(crate) fn mark_failed(&mut self, subtask_id: &str, reason: String) -> Result<(), String> {
        self.state = BoardState::Failed;
        self.updated_at = now_string();
        let card = self.subtask_mut(subtask_id)?;
        card.status = SubtaskState::Failed;
        card.latest_review = Some(reason);
        Ok(())
    }

    pub(crate) fn complete_if_finished(&mut self) {
        if self.subtasks.iter().all(|card| matches!(card.status, SubtaskState::Done)) {
            self.state = BoardState::Completed;
            self.active_subtask_id = None;
            self.updated_at = now_string();
        }
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

pub(crate) fn change_log_entries(workspace: &str) -> Vec<String> {
    let log_path = Path::new(workspace).join("change.log");
    let Ok(content) = std::fs::read_to_string(log_path) else {
        return Vec::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn extract_paths(entries: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    for entry in entries {
        let Some(path) = entry
            .strip_prefix("CREATE: ")
            .or_else(|| entry.strip_prefix("MODIFY: "))
        else {
            continue;
        };
        let path = path.trim().to_string();
        if seen.insert(path.clone()) {
            files.push(path);
        }
    }
    files
}

pub(crate) fn relative_paths(workspace: &str, paths: &[String]) -> Vec<String> {
    let root = Path::new(workspace);
    paths.iter()
        .map(|path| {
            let pb = PathBuf::from(path);
            pb.strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.clone())
        })
        .collect()
}

fn parse_plan_subtasks(plan: &str) -> Vec<SubtaskCard> {
    plan.lines()
        .filter_map(parse_checklist_line)
        .collect()
}

fn parse_checklist_line(line: &str) -> Option<SubtaskCard> {
    let trimmed = line.trim();
    if !trimmed.starts_with("- [") || !trimmed.contains("**") {
        return None;
    }

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
        status: SubtaskState::Pending,
        attempts: 0,
        latest_implementation: None,
        latest_review: None,
        review_findings: Vec::new(),
        files_touched: Vec::new(),
    })
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
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "Demo".to_string(),
                description: "desc".to_string(),
                kind: SubtaskKind::Feature,
                status: SubtaskState::Pending,
                attempts: 0,
                latest_implementation: None,
                latest_review: None,
                review_findings: Vec::new(),
                files_touched: Vec::new(),
            }],
            updated_at: "now".to_string(),
        };

        let markdown = board.render_markdown();
        assert!(markdown.contains("only shared coordination state"));
        assert!(markdown.contains("### F1. Demo"));
    }
}
