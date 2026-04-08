/// Markdown rendering and display helpers for the shared blackboard.
///
/// Provides `Blackboard::render_markdown()` and label functions used
/// to format board/subtask state for the BLACKBOARD.md output.
use super::blackboard::{Blackboard, BoardState, SubtaskKind, SubtaskState};
use crate::planning_schema::SuggestedSkill;

// ── Markdown rendering ──────────────────────────────────────────��─────────

impl Blackboard {
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
}

// ── Label helpers ─────────────────────────────────────────────────────────

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
        SuggestedSkill::UiDesignSystem => "ui-design-system",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::blackboard::*;

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
}
