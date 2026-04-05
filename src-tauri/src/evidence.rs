use crate::skills::blackboard::{
    Blackboard, BoardState, SubtaskCard, SubtaskKind, SubtaskState, BLACKBOARD_JSON, BLACKBOARD_MD,
};
use crate::verifier::VERIFIER_RESULT_JSON;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub(crate) const BLACKBOARD_EVENTS_JSONL: &str = "BLACKBOARD_EVENTS.jsonl";
pub(crate) const EVIDENCE_INDEX_JSON: &str = "EVIDENCE_INDEX.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct EvidenceEvent {
    pub ts: u64,
    pub event_type: String,
    pub agent: String,
    #[serde(default)]
    pub subtask_id: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct EvidenceIndex {
    pub generated_at: String,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub board_state: Option<String>,
    pub total_events: usize,
    pub project_artifacts: Vec<String>,
    pub recent_events: Vec<EvidenceEvent>,
    pub subtasks: Vec<SubtaskEvidence>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubtaskEvidence {
    pub subtask_id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub attempts: u32,
    pub depends_on: Vec<String>,
    pub files_touched: Vec<String>,
    pub review_findings: Vec<String>,
    #[serde(default)]
    pub latest_implementation: Option<String>,
    #[serde(default)]
    pub latest_review: Option<String>,
    pub event_count: usize,
    #[serde(default)]
    pub last_event_type: Option<String>,
    pub recent_event_summaries: Vec<String>,
    pub artifacts: Vec<String>,
}

pub(crate) fn record_event(workspace: &str, event: EvidenceEvent) -> Result<(), String> {
    append_event(workspace, &event)?;
    refresh_evidence_index(workspace)?;
    Ok(())
}

pub(crate) fn refresh_evidence_index(workspace: &str) -> Result<(), String> {
    let board = read_blackboard(workspace)?;
    let events = read_events(workspace)?;
    let index = build_evidence_index(workspace, board.as_ref(), &events);
    let path = Path::new(workspace).join(EVIDENCE_INDEX_JSON);
    let json = serde_json::to_string_pretty(&index)
        .map_err(|e| format!("Cannot serialize {EVIDENCE_INDEX_JSON}: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(())
}

pub(crate) fn read_evidence_index(workspace: &str) -> Result<Option<EvidenceIndex>, String> {
    let path = Path::new(workspace).join(EVIDENCE_INDEX_JSON);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let index =
        serde_json::from_str(&text).map_err(|e| format!("Cannot parse {}: {e}", path.display()))?;
    Ok(Some(index))
}

fn append_event(workspace: &str, event: &EvidenceEvent) -> Result<(), String> {
    let path = Path::new(workspace).join(BLACKBOARD_EVENTS_JSONL);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let line = serde_json::to_string(event)
        .map_err(|e| format!("Cannot serialize {BLACKBOARD_EVENTS_JSONL} line: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("Cannot append {}: {e}", path.display()))
}

fn read_blackboard(workspace: &str) -> Result<Option<Blackboard>, String> {
    let path = Path::new(workspace).join(BLACKBOARD_JSON);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let board =
        serde_json::from_str(&text).map_err(|e| format!("Cannot parse {}: {e}", path.display()))?;
    Ok(Some(board))
}

fn read_events(workspace: &str) -> Result<Vec<EvidenceEvent>, String> {
    let path = Path::new(workspace).join(BLACKBOARD_EVENTS_JSONL);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<EvidenceEvent>(line)
                .map_err(|e| format!("Cannot parse {} line: {e}", path.display()))
        })
        .collect()
}

fn build_evidence_index(
    workspace: &str,
    board: Option<&Blackboard>,
    events: &[EvidenceEvent],
) -> EvidenceIndex {
    let recent_events = events
        .iter()
        .rev()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    let subtasks = board
        .map(|board| {
            board
                .subtasks
                .iter()
                .map(|card| build_subtask_evidence(workspace, card, events))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    EvidenceIndex {
        generated_at: Utc::now().to_rfc3339(),
        task: board.map(|board| board.task.clone()),
        board_state: board.map(|board| board_state_label(&board.state).to_string()),
        total_events: events.len(),
        project_artifacts: collect_project_artifacts(workspace),
        recent_events,
        subtasks,
    }
}

fn build_subtask_evidence(
    workspace: &str,
    card: &SubtaskCard,
    events: &[EvidenceEvent],
) -> SubtaskEvidence {
    let subtask_events = events
        .iter()
        .filter(|event| event.subtask_id.as_deref() == Some(card.id.as_str()))
        .collect::<Vec<_>>();
    let recent_event_summaries = subtask_events
        .iter()
        .rev()
        .take(5)
        .map(|event| event.summary.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let last_event_type = subtask_events.last().map(|event| event.event_type.clone());

    SubtaskEvidence {
        subtask_id: card.id.clone(),
        title: card.title.clone(),
        kind: subtask_kind_label(&card.kind).to_string(),
        status: subtask_state_label(&card.status).to_string(),
        attempts: card.attempts,
        depends_on: card.depends_on.clone(),
        files_touched: card.files_touched.clone(),
        review_findings: card.review_findings.clone(),
        latest_implementation: card.latest_implementation.clone(),
        latest_review: card.latest_review.clone(),
        event_count: subtask_events.len(),
        last_event_type,
        recent_event_summaries,
        artifacts: collect_subtask_artifacts(workspace, card, &subtask_events),
    }
}

fn collect_subtask_artifacts(
    workspace: &str,
    card: &SubtaskCard,
    subtask_events: &[&EvidenceEvent],
) -> Vec<String> {
    let mut artifacts = card.files_touched.clone();
    for event in subtask_events {
        for artifact in &event.artifacts {
            if artifact == VERIFIER_RESULT_JSON || is_coordination_artifact(artifact) {
                continue;
            }
            artifacts.push(artifact.clone());
        }
    }
    if card.merge_conflict.is_some() {
        artifacts.push(BLACKBOARD_MD.to_string());
    }
    artifacts.extend(collect_verifier_archive_artifacts(workspace, &card.id));
    artifacts.sort();
    artifacts.dedup();
    artifacts
}

fn collect_verifier_archive_artifacts(workspace: &str, subtask_id: &str) -> Vec<String> {
    let archive_dir = Path::new(workspace)
        .join(".ai-dev-hub")
        .join("verifier")
        .join(subtask_id);
    let Ok(entries) = std::fs::read_dir(&archive_dir) else {
        return Vec::new();
    };

    let mut artifacts = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            path.strip_prefix(workspace)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    artifacts
}

fn is_coordination_artifact(path: &str) -> bool {
    matches!(path, BLACKBOARD_JSON | BLACKBOARD_MD | "PLAN.md")
}

fn collect_project_artifacts(workspace: &str) -> Vec<String> {
    let root = Path::new(workspace);
    let mut artifacts = [
        "PLAN.md",
        "PLAN_GRAPH.json",
        "PLAN_ACCEPTANCE.json",
        BLACKBOARD_JSON,
        BLACKBOARD_MD,
        BLACKBOARD_EVENTS_JSONL,
        EVIDENCE_INDEX_JSON,
        "PLAN_BLACKBOARD.md",
        "PLAN_BLACKBOARD.json",
        "bugs.md",
        "test.md",
        "security.md",
        "PROJECT_REPORT.md",
        "change.log",
    ]
    .into_iter()
    .filter(|path| root.join(path).exists())
    .map(str::to_string)
    .collect::<Vec<_>>();
    artifacts.sort();
    if root.join(".ai-dev-hub/verifier").exists() {
        artifacts.push(".ai-dev-hub/verifier".to_string());
    }
    artifacts.sort();
    artifacts.dedup();
    artifacts
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
    use crate::skills::blackboard::{BoardState, SubtaskCard, SubtaskKind, SubtaskState};

    fn sample_board() -> Blackboard {
        Blackboard {
            task: "demo".to_string(),
            state: BoardState::InProgress,
            active_subtask_id: Some("F1".to_string()),
            active_subtask_ids: vec!["F1".to_string()],
            subtasks: vec![SubtaskCard {
                id: "F1".to_string(),
                title: "Jobs API".to_string(),
                description: "Build endpoints".to_string(),
                kind: SubtaskKind::Feature,
                depends_on: Vec::new(),
                can_run_in_parallel: true,
                parallel_group: None,
                suggested_skill: None,
                expected_touch: vec!["api/jobs".to_string()],
                status: SubtaskState::NeedsFix,
                attempts: 2,
                latest_implementation: Some("Implemented CRUD".to_string()),
                latest_review: Some("Missing validation".to_string()),
                review_findings: vec!["Add 4xx validation handling".to_string()],
                files_touched: vec!["src/jobs.rs".to_string()],
                isolated_workspace: None,
                merge_conflict: None,
            }],
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn refresh_evidence_index_writes_index() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let board = sample_board();
        std::fs::write(
            dir.path().join(BLACKBOARD_JSON),
            serde_json::to_string_pretty(&board).unwrap(),
        )
        .unwrap();
        let verifier_dir = dir.path().join(".ai-dev-hub").join("verifier").join("F1");
        std::fs::create_dir_all(&verifier_dir).unwrap();
        std::fs::write(verifier_dir.join("attempt-2.json"), "{}").unwrap();
        record_event(
            workspace,
            EvidenceEvent {
                ts: 1,
                event_type: "review_failed".to_string(),
                agent: "codex".to_string(),
                subtask_id: Some("F1".to_string()),
                summary: "Validation missing".to_string(),
                artifacts: vec![
                    "BLACKBOARD.md".to_string(),
                    "notes/review.md".to_string(),
                    VERIFIER_RESULT_JSON.to_string(),
                ],
            },
        )
        .unwrap();

        let index = read_evidence_index(workspace).unwrap().unwrap();
        assert_eq!(index.total_events, 1);
        assert_eq!(index.subtasks.len(), 1);
        assert_eq!(index.subtasks[0].subtask_id, "F1");
        assert_eq!(index.subtasks[0].event_count, 1);
        assert_eq!(
            index.subtasks[0].last_event_type.as_deref(),
            Some("review_failed")
        );
        assert!(index.subtasks[0]
            .artifacts
            .contains(&"notes/review.md".to_string()));
        assert!(index.subtasks[0]
            .artifacts
            .contains(&".ai-dev-hub/verifier/F1/attempt-2.json".to_string()));
    }

    #[test]
    fn collect_project_artifacts_only_lists_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("PLAN.md"), "plan").unwrap();
        std::fs::write(dir.path().join("bugs.md"), "bugs").unwrap();

        let artifacts = collect_project_artifacts(dir.path().to_str().unwrap());
        assert_eq!(
            artifacts,
            vec!["PLAN.md".to_string(), "bugs.md".to_string()]
        );
    }
}
