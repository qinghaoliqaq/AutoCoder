/// Plan parsing logic for the shared blackboard.
///
/// Parses PLAN.md checklist items and PLAN_GRAPH.json into SubtaskCard
/// structs used by the blackboard to track subtask execution.
use super::blackboard::{SubtaskCard, SubtaskKind, SubtaskState};
use crate::planning_schema::{read_plan_graph, PlanSubtask, SubtaskCategory};
use std::collections::HashSet;

// ── Public helpers (called from Blackboard::load_or_create) ───────────────

pub(super) fn build_initial_subtasks(workspace: &str, plan: &str) -> Vec<SubtaskCard> {
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

// ── Plan parsing ──────────────────────────────────────────────────────────

pub(super) fn parse_plan_subtasks(plan: &str) -> Vec<SubtaskCard> {
    plan.lines().filter_map(parse_checklist_line).collect()
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
        attempted_fixes: Vec::new(),
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
        attempted_fixes: Vec::new(),
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

// ── Tests ─────────────────────────────────────────────────────────────────

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
    fn load_or_create_prefers_plan_graph_when_available() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        std::fs::write(
            dir.path().join(".ai-dev-hub/PLAN.md"),
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

        use super::super::blackboard::Blackboard;
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
}
