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

/// Compact, LLM-friendly digest of project evidence for injection into prompts.
/// Unlike the full EvidenceIndex JSON dump, this is a short markdown summary
/// designed to fit in a system prompt without overwhelming context.
pub(crate) fn build_evidence_digest(workspace: &str) -> Option<String> {
    let _ = refresh_evidence_index(workspace);
    let index = read_evidence_index(workspace).ok()??;

    let mut lines = Vec::new();
    lines.push("## Project Evidence Digest".to_string());
    lines.push(String::new());

    // Overall status
    if let Some(task) = &index.task {
        lines.push(format!("**Task:** {task}"));
    }
    if let Some(state) = &index.board_state {
        lines.push(format!("**Board state:** {state}"));
    }
    lines.push(format!("**Total events:** {}", index.total_events));

    // Aggregate statistics
    let total = index.subtasks.len();
    if total > 0 {
        let done = index.subtasks.iter().filter(|s| s.status == "done").count();
        let failed = index.subtasks.iter().filter(|s| s.status == "failed").count();
        let in_progress = index.subtasks.iter().filter(|s| s.status == "in_progress").count();
        let needs_fix = index.subtasks.iter().filter(|s| s.status == "needs_fix").count();
        let pending = index.subtasks.iter().filter(|s| s.status == "pending").count();
        let total_attempts: u32 = index.subtasks.iter().map(|s| s.attempts).sum();
        let multi_attempt: Vec<_> = index.subtasks.iter().filter(|s| s.attempts > 1).collect();

        lines.push(format!(
            "**Subtasks:** {done}/{total} done, {failed} failed, {in_progress} active, {needs_fix} needs_fix, {pending} pending"
        ));
        if total_attempts > total as u32 {
            lines.push(format!(
                "**Total attempts:** {total_attempts} (avg {:.1}/subtask)",
                total_attempts as f64 / total as f64
            ));
        }

        // Highlight trouble spots — subtasks that needed multiple attempts
        if !multi_attempt.is_empty() {
            lines.push(String::new());
            lines.push("### Trouble spots (multiple attempts)".to_string());
            for sub in &multi_attempt {
                lines.push(format!(
                    "- **{}** ({}): {} attempts, status: {}",
                    sub.subtask_id, sub.title, sub.attempts, sub.status
                ));
                if !sub.review_findings.is_empty() {
                    for finding in sub.review_findings.iter().take(3) {
                        lines.push(format!("  - {finding}"));
                    }
                }
                if let Some(review) = &sub.latest_review {
                    lines.push(format!("  - Last review: {review}"));
                }
            }
        }

        // Failed subtasks
        let failed_subs: Vec<_> = index.subtasks.iter().filter(|s| s.status == "failed").collect();
        if !failed_subs.is_empty() {
            lines.push(String::new());
            lines.push("### Failed subtasks".to_string());
            for sub in &failed_subs {
                lines.push(format!("- **{}** ({}): {}", sub.subtask_id, sub.title, sub.status));
                for summary in sub.recent_event_summaries.iter().rev().take(2) {
                    lines.push(format!("  - {summary}"));
                }
            }
        }
    }

    // Recent events timeline (last 10)
    if !index.recent_events.is_empty() {
        lines.push(String::new());
        lines.push("### Recent activity".to_string());
        for event in index.recent_events.iter().rev().take(10) {
            let subtask = event.subtask_id.as_deref().unwrap_or("project");
            lines.push(format!(
                "- [{}] {}: {}",
                subtask, event.event_type, event.summary
            ));
        }
    }

    // QA history — extract previous QA verdicts for multi-round reasoning
    let qa_events: Vec<_> = index
        .recent_events
        .iter()
        .filter(|e| e.event_type.starts_with("qa_"))
        .collect();
    if !qa_events.is_empty() {
        lines.push(String::new());
        lines.push("### QA history".to_string());
        for event in &qa_events {
            lines.push(format!("- **{}**: {}", event.event_type, event.summary));
        }
    }

    if lines.len() <= 2 {
        return None; // Nothing meaningful to report
    }

    Some(lines.join("\n"))
}

/// Build a focused context block for a specific subtask, summarizing its
/// history of attempts, failures, review findings, and verifier results.
/// Injected into the Claude prompt when retrying a subtask so the LLM knows
/// what went wrong previously.
pub(crate) fn build_subtask_context(workspace: &str, subtask_id: &str) -> Option<String> {
    let index = read_evidence_index(workspace).ok()??;
    let sub = index.subtasks.iter().find(|s| s.subtask_id == subtask_id)?;

    if sub.attempts <= 1 && sub.review_findings.is_empty() {
        return None; // First attempt, no history to inject
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "## Previous history for subtask {} ({})",
        sub.subtask_id, sub.title
    ));
    lines.push(format!(
        "This is attempt {}. Previous attempts encountered these issues:",
        sub.attempts + 1
    ));

    if !sub.review_findings.is_empty() {
        lines.push(String::new());
        lines.push("**Review findings to address:**".to_string());
        for finding in &sub.review_findings {
            lines.push(format!("- {finding}"));
        }
    }

    if let Some(review) = &sub.latest_review {
        lines.push(format!("\n**Latest review:** {review}"));
    }

    if let Some(impl_note) = &sub.latest_implementation {
        lines.push(format!("\n**Previous implementation note:** {impl_note}"));
    }

    if !sub.recent_event_summaries.is_empty() {
        lines.push(String::new());
        lines.push("**Event timeline:**".to_string());
        for summary in &sub.recent_event_summaries {
            lines.push(format!("- {summary}"));
        }
    }

    lines.push(String::new());
    lines.push("Use this history to avoid repeating the same mistakes. Fix the identified issues while preserving what worked.".to_string());

    Some(lines.join("\n"))
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

    #[test]
    fn evidence_digest_includes_trouble_spots() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = sample_board();
        board.subtasks[0].attempts = 3;
        std::fs::write(
            dir.path().join(BLACKBOARD_JSON),
            serde_json::to_string_pretty(&board).unwrap(),
        )
        .unwrap();
        record_event(
            workspace,
            EvidenceEvent {
                ts: 1,
                event_type: "needs_fix".to_string(),
                agent: "codex".to_string(),
                subtask_id: Some("F1".to_string()),
                summary: "Validation missing".to_string(),
                artifacts: Vec::new(),
            },
        )
        .unwrap();
        record_event(
            workspace,
            EvidenceEvent {
                ts: 2,
                event_type: "qa_failed".to_string(),
                agent: "claude".to_string(),
                subtask_id: None,
                summary: "API tests fail".to_string(),
                artifacts: Vec::new(),
            },
        )
        .unwrap();

        let digest = build_evidence_digest(workspace).expect("should produce digest");
        assert!(digest.contains("Trouble spots"), "should have trouble spots section");
        assert!(digest.contains("F1"), "should mention subtask F1");
        assert!(digest.contains("3 attempts"), "should show attempt count");
        assert!(digest.contains("QA history"), "should have QA history section");
        assert!(digest.contains("qa_failed"), "should show QA failure");
    }

    #[test]
    fn subtask_context_returns_none_for_first_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = sample_board();
        board.subtasks[0].attempts = 1;
        board.subtasks[0].review_findings.clear();
        std::fs::write(
            dir.path().join(BLACKBOARD_JSON),
            serde_json::to_string_pretty(&board).unwrap(),
        )
        .unwrap();
        refresh_evidence_index(workspace).unwrap();

        assert!(build_subtask_context(workspace, "F1").is_none());
    }

    #[test]
    fn subtask_context_includes_findings_on_retry() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        let board = sample_board(); // attempts=2, has review_findings
        std::fs::write(
            dir.path().join(BLACKBOARD_JSON),
            serde_json::to_string_pretty(&board).unwrap(),
        )
        .unwrap();
        refresh_evidence_index(workspace).unwrap();

        let ctx = build_subtask_context(workspace, "F1").expect("should produce context");
        assert!(ctx.contains("Previous history"), "should have header");
        assert!(ctx.contains("4xx validation"), "should include review finding");
        assert!(ctx.contains("Missing validation"), "should include latest review");
    }
}
