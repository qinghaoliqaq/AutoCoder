use super::blackboard::{
    Blackboard, BoardState, SubtaskCard, SubtaskKind, SubtaskState, BLACKBOARD_JSON, BLACKBOARD_MD,
};
use super::planning_schema::{PLAN_ACCEPTANCE_JSON, PLAN_GRAPH_JSON};
use super::verifier::VERIFIER_RESULT_JSON;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub(crate) const BLACKBOARD_EVENTS_JSONL: &str = ".ai-dev-hub/BLACKBOARD_EVENTS.jsonl";
pub(crate) const EVIDENCE_INDEX_JSON: &str = ".ai-dev-hub/EVIDENCE_INDEX.json";

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

/// Quantitative metrics derived from evidence, used for data-driven QA decisions.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct EvidenceMetrics {
    pub subtask_total: usize,
    pub subtask_done: usize,
    pub subtask_failed: usize,
    pub subtask_in_progress: usize,
    pub subtask_needs_fix: usize,
    pub subtask_pending: usize,
    /// Fraction of subtasks that are done (0.0 - 1.0).
    pub completion_ratio: f64,
    /// Total attempts across all subtasks.
    pub total_attempts: u32,
    /// Average attempts per subtask.
    pub avg_attempts: f64,
    /// Number of subtasks that required more than one attempt.
    pub multi_attempt_count: usize,
    /// Total evidence events recorded.
    pub total_events: usize,
    /// Number of review phases that passed.
    pub review_passed: usize,
    /// Number of review phases that failed.
    pub review_failed: usize,
    /// Number of test phases that passed.
    pub test_passed: usize,
    /// Number of test phases that failed.
    pub test_failed: usize,
    /// Number of QA runs that passed.
    pub qa_passed: usize,
    /// Number of QA runs that failed.
    pub qa_failed: usize,
    /// Whether a plan_completed event exists.
    pub plan_completed: bool,
    /// Whether a debug session was run.
    pub debug_sessions: usize,
    /// Overall health score (0-100), computed from sub-metrics.
    pub health_score: u32,
}

/// Compute quantitative metrics from the evidence index and raw events.
pub(crate) fn compute_evidence_metrics(workspace: &str) -> Option<EvidenceMetrics> {
    // Only produce metrics when a real BLACKBOARD.json exists; otherwise
    // empty workspaces would generate zero-value metrics that pollute QA prompts.
    let board = read_blackboard(workspace).ok()?;
    if board.is_none() {
        return None;
    }
    let _ = refresh_evidence_index(workspace);
    let index = read_evidence_index(workspace).ok()??;
    let events = read_events(workspace).ok()?;

    let subtask_total = index.subtasks.len();
    let subtask_done = index.subtasks.iter().filter(|s| s.status == "done").count();
    let subtask_failed = index
        .subtasks
        .iter()
        .filter(|s| s.status == "failed")
        .count();
    let subtask_in_progress = index
        .subtasks
        .iter()
        .filter(|s| s.status == "in_progress")
        .count();
    let subtask_needs_fix = index
        .subtasks
        .iter()
        .filter(|s| s.status == "needs_fix")
        .count();
    let subtask_pending = index
        .subtasks
        .iter()
        .filter(|s| s.status == "pending")
        .count();

    let completion_ratio = if subtask_total > 0 {
        subtask_done as f64 / subtask_total as f64
    } else {
        0.0
    };

    let total_attempts: u32 = index.subtasks.iter().map(|s| s.attempts).sum();
    let avg_attempts = if subtask_total > 0 {
        total_attempts as f64 / subtask_total as f64
    } else {
        0.0
    };
    let multi_attempt_count = index.subtasks.iter().filter(|s| s.attempts > 1).count();

    let review_passed = events
        .iter()
        .filter(|e| e.event_type.starts_with("review_") && e.event_type.ends_with("_passed"))
        .count();
    let review_failed = events
        .iter()
        .filter(|e| e.event_type.starts_with("review_") && e.event_type.ends_with("_failed"))
        .count();
    let test_passed = events
        .iter()
        .filter(|e| e.event_type.starts_with("test_") && e.event_type.ends_with("_passed"))
        .count();
    let test_failed = events
        .iter()
        .filter(|e| e.event_type.starts_with("test_") && e.event_type.ends_with("_failed"))
        .count();
    let qa_passed = events
        .iter()
        .filter(|e| matches!(e.event_type.as_str(), "qa_passed" | "qa_pass_with_concerns"))
        .count();
    let qa_failed = events
        .iter()
        .filter(|e| e.event_type == "qa_failed")
        .count();
    let plan_completed = events.iter().any(|e| e.event_type == "plan_completed");
    let debug_sessions = events
        .iter()
        .filter(|e| e.event_type == "debug_completed")
        .count();

    // Health score: weighted composite
    let completion_score = (completion_ratio * 40.0) as u32; // 40 points max
    let failure_penalty = (subtask_failed as u32).min(20) * 2; // -2 per failure, max -40
    let review_score = if review_passed + review_failed > 0 {
        ((review_passed as f64 / (review_passed + review_failed) as f64) * 20.0) as u32
    } else {
        10 // neutral if no reviews
    };
    let test_score = if test_passed + test_failed > 0 {
        ((test_passed as f64 / (test_passed + test_failed) as f64) * 20.0) as u32
    } else {
        10 // neutral if no tests
    };
    let attempt_penalty = if avg_attempts > 2.0 {
        ((avg_attempts - 2.0) * 5.0).min(10.0) as u32
    } else {
        0
    };
    let plan_bonus = if plan_completed { 10 } else { 0 };

    let raw_score = completion_score + review_score + test_score + plan_bonus;
    let health_score = raw_score
        .saturating_sub(failure_penalty + attempt_penalty)
        .min(100);

    Some(EvidenceMetrics {
        subtask_total,
        subtask_done,
        subtask_failed,
        subtask_in_progress,
        subtask_needs_fix,
        subtask_pending,
        completion_ratio,
        total_attempts,
        avg_attempts,
        multi_attempt_count,
        total_events: events.len(),
        review_passed,
        review_failed,
        test_passed,
        test_failed,
        qa_passed,
        qa_failed,
        plan_completed,
        debug_sessions,
        health_score,
    })
}

/// Format evidence metrics as a concise markdown section for prompt injection.
pub(crate) fn format_metrics_section(metrics: &EvidenceMetrics) -> String {
    let mut lines = Vec::new();
    lines.push("## Quantitative Evidence Metrics".to_string());
    lines.push(String::new());
    lines.push(format!("| Metric | Value |"));
    lines.push(format!("|--------|-------|"));
    lines.push(format!(
        "| Subtask completion | {}/{} ({:.0}%) |",
        metrics.subtask_done,
        metrics.subtask_total,
        metrics.completion_ratio * 100.0
    ));
    lines.push(format!("| Subtasks failed | {} |", metrics.subtask_failed));
    lines.push(format!(
        "| Subtasks needs_fix | {} |",
        metrics.subtask_needs_fix
    ));
    lines.push(format!(
        "| Subtasks pending | {} |",
        metrics.subtask_pending
    ));
    lines.push(format!(
        "| Subtasks in_progress | {} |",
        metrics.subtask_in_progress
    ));
    lines.push(format!(
        "| Total attempts | {} (avg {:.1}/subtask) |",
        metrics.total_attempts, metrics.avg_attempts
    ));
    lines.push(format!(
        "| Multi-attempt subtasks | {} |",
        metrics.multi_attempt_count
    ));
    lines.push(format!(
        "| Review phases passed/failed | {}/{} |",
        metrics.review_passed, metrics.review_failed
    ));
    lines.push(format!(
        "| Test phases passed/failed | {}/{} |",
        metrics.test_passed, metrics.test_failed
    ));
    lines.push(format!(
        "| Previous QA passed/failed | {}/{} |",
        metrics.qa_passed, metrics.qa_failed
    ));
    lines.push(format!(
        "| Plan completed | {} |",
        if metrics.plan_completed { "yes" } else { "no" }
    ));
    lines.push(format!(
        "| Debug sessions run | {} |",
        metrics.debug_sessions
    ));
    lines.push(format!(
        "| Total evidence events | {} |",
        metrics.total_events
    ));
    lines.push(format!(
        "| **Health score** | **{}/100** |",
        metrics.health_score
    ));
    lines.push(String::new());
    lines.push("### Scoring guidance".to_string());
    lines.push(String::new());
    lines.push("Use the metrics above to ground your QA verdict:".to_string());
    lines.push("- **completion_ratio < 1.0** with pending/in_progress subtasks → likely FAIL (incomplete work)".to_string());
    lines.push(
        "- **subtask_failed > 0** → FAIL unless failures are non-blocking and documented"
            .to_string(),
    );
    lines.push(
        "- **health_score >= 80** with no failed subtasks → strong PASS candidate".to_string(),
    );
    lines.push(
        "- **health_score 50-79** → PASS_WITH_CONCERNS or FAIL depending on severity".to_string(),
    );
    lines.push("- **health_score < 50** → likely FAIL".to_string());
    lines.push("- **review_failed > 0 or test_failed > 0** → weigh against pass unless issues were fixed in later events".to_string());
    lines.join("\n")
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
        let failed = index
            .subtasks
            .iter()
            .filter(|s| s.status == "failed")
            .count();
        let in_progress = index
            .subtasks
            .iter()
            .filter(|s| s.status == "in_progress")
            .count();
        let needs_fix = index
            .subtasks
            .iter()
            .filter(|s| s.status == "needs_fix")
            .count();
        let pending = index
            .subtasks
            .iter()
            .filter(|s| s.status == "pending")
            .count();
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
        let failed_subs: Vec<_> = index
            .subtasks
            .iter()
            .filter(|s| s.status == "failed")
            .collect();
        if !failed_subs.is_empty() {
            lines.push(String::new());
            lines.push("### Failed subtasks".to_string());
            for sub in &failed_subs {
                lines.push(format!(
                    "- **{}** ({}): {}",
                    sub.subtask_id, sub.title, sub.status
                ));
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
        sub.attempts
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
    // Index refresh is best-effort — the event is already appended.
    // Concurrent BLACKBOARD.json writes can make read_blackboard fail
    // transiently, which must not bubble up.
    if let Err(e) = refresh_evidence_index(workspace) {
        tracing::warn!("Evidence index refresh failed (non-fatal): {e}");
    }
    Ok(())
}

pub(crate) fn refresh_evidence_index(workspace: &str) -> Result<(), String> {
    let board = read_blackboard(workspace)?;
    let events = read_events(workspace)?;
    let index = build_evidence_index(workspace, board.as_ref(), &events);
    let path = Path::new(workspace).join(EVIDENCE_INDEX_JSON);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&index)
        .map_err(|e| format!("Cannot serialize {EVIDENCE_INDEX_JSON}: {e}"))?;
    // Atomic write-to-temp-then-rename so concurrent readers never see partial content.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("Cannot rename {} → {}: {e}", tmp.display(), path.display()))?;
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

/// Maximum JSONL file size before truncation (keep last N lines).
const MAX_EVENTS_FILE_BYTES: u64 = 512 * 1024; // 512 KB
const TRUNCATE_KEEP_LINES: usize = 200;

fn append_event(workspace: &str, event: &EvidenceEvent) -> Result<(), String> {
    let path = Path::new(workspace).join(BLACKBOARD_EVENTS_JSONL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    let mut line = serde_json::to_string(event)
        .map_err(|e| format!("Cannot serialize {BLACKBOARD_EVENTS_JSONL} line: {e}"))?;
    line.push('\n');

    // Open the file for both read+write so we can truncate-then-append in one
    // session.  Hold the file open for the whole operation so that the
    // truncate and append are not interleaved by concurrent callers.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;

    // Truncate the file if it's grown too large, keeping only recent events.
    if let Ok(meta) = file.metadata() {
        if meta.len() > MAX_EVENTS_FILE_BYTES {
            truncate_events_file_locked(&file);
        }
    }

    // Seek to end and append.
    use std::io::{Seek, SeekFrom};
    let mut file = file;
    let _ = file.seek(SeekFrom::End(0));
    // Use a single write_all so the JSON + newline cannot be split across
    // two syscalls — prevents interleaving when parallel subtasks append
    // to the same JSONL file concurrently.
    file.write_all(line.as_bytes())
        .map_err(|e| format!("Cannot append {}: {e}", path.display()))
}

/// Truncate the events JSONL file in-place to the last N lines.
/// Called while holding the file handle open so the truncate and rewrite
/// are not interleaved by concurrent callers.
///
/// Uses raw bytes (not `read_to_string`) to handle potentially corrupted
/// content from concurrent append races — invalid UTF-8 lines are discarded
/// rather than causing the entire truncation to fail.
fn truncate_events_file_locked(file: &std::fs::File) {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = file;
    if file.seek(SeekFrom::Start(0)).is_err() {
        return;
    }
    let mut raw = Vec::new();
    if file.read_to_end(&mut raw).is_err() {
        return;
    }
    // Split by newline bytes and keep only valid UTF-8 lines.
    let lines: Vec<&str> = raw
        .split(|&b| b == b'\n')
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .filter(|s| !s.is_empty())
        .collect();
    if lines.len() <= TRUNCATE_KEEP_LINES {
        return;
    }
    let kept = &lines[lines.len() - TRUNCATE_KEEP_LINES..];
    let mut content = kept.join("\n");
    content.push('\n');
    let _ = file.seek(SeekFrom::Start(0));
    let _ = file.set_len(0);
    let _ = file.write_all(content.as_bytes());
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
    // Tolerate malformed lines — concurrent parallel subtask writes can
    // corrupt a line (e.g. two JSON objects on one line).  Skipping bad
    // lines is better than failing every subsequent subtask.
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<EvidenceEvent>(line) {
            Ok(event) => Some(event),
            Err(e) => {
                tracing::warn!("Skipping malformed line in {}: {e}", path.display());
                None
            }
        })
        .collect())
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
    matches!(
        path,
        BLACKBOARD_JSON | BLACKBOARD_MD | ".ai-dev-hub/PLAN.md"
    )
}

fn collect_project_artifacts(workspace: &str) -> Vec<String> {
    let root = Path::new(workspace);
    let mut artifacts = [
        ".ai-dev-hub/PLAN.md",
        PLAN_GRAPH_JSON,
        PLAN_ACCEPTANCE_JSON,
        BLACKBOARD_JSON,
        BLACKBOARD_MD,
        BLACKBOARD_EVENTS_JSONL,
        EVIDENCE_INDEX_JSON,
        ".ai-dev-hub/PLAN_BLACKBOARD.md",
        ".ai-dev-hub/PLAN_BLACKBOARD.json",
        ".ai-dev-hub/bugs.md",
        ".ai-dev-hub/test.md",
        ".ai-dev-hub/security.md",
        ".ai-dev-hub/PROJECT_REPORT.md",
        ".ai-dev-hub/change.log",
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
    use super::super::blackboard::{BoardState, SubtaskCard, SubtaskKind, SubtaskState};
    use super::*;

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
                attempted_fixes: Vec::new(),
            }],
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn refresh_evidence_index_writes_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
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
                    ".ai-dev-hub/BLACKBOARD.md".to_string(),
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
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        std::fs::write(dir.path().join(".ai-dev-hub/PLAN.md"), "plan").unwrap();
        std::fs::write(dir.path().join(".ai-dev-hub/bugs.md"), "bugs").unwrap();

        let artifacts = collect_project_artifacts(dir.path().to_str().unwrap());
        assert_eq!(
            artifacts,
            vec![
                ".ai-dev-hub/PLAN.md".to_string(),
                ".ai-dev-hub/bugs.md".to_string()
            ]
        );
    }

    #[test]
    fn evidence_digest_includes_trouble_spots() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
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
        assert!(
            digest.contains("Trouble spots"),
            "should have trouble spots section"
        );
        assert!(digest.contains("F1"), "should mention subtask F1");
        assert!(digest.contains("3 attempts"), "should show attempt count");
        assert!(
            digest.contains("QA history"),
            "should have QA history section"
        );
        assert!(digest.contains("qa_failed"), "should show QA failure");
    }

    #[test]
    fn subtask_context_returns_none_for_first_attempt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
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
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
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
        assert!(
            ctx.contains("4xx validation"),
            "should include review finding"
        );
        assert!(
            ctx.contains("Missing validation"),
            "should include latest review"
        );
    }

    #[test]
    fn compute_metrics_from_board_and_events() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        let workspace = dir.path().to_str().unwrap();
        let mut board = sample_board();
        board.subtasks[0].status = SubtaskState::Done;
        board.subtasks[0].attempts = 2;
        std::fs::write(
            dir.path().join(BLACKBOARD_JSON),
            serde_json::to_string_pretty(&board).unwrap(),
        )
        .unwrap();

        // Record a mix of events
        for (event_type, agent) in &[
            ("plan_completed", "system"),
            ("review_plan_check_passed", "system"),
            ("review_security_failed", "system"),
            ("test_integration_test_passed", "system"),
            ("test_gen_test_plan_passed", "system"),
            ("qa_failed", "claude"),
        ] {
            record_event(
                workspace,
                EvidenceEvent {
                    ts: 1,
                    event_type: event_type.to_string(),
                    agent: agent.to_string(),
                    subtask_id: None,
                    summary: "test".to_string(),
                    artifacts: Vec::new(),
                },
            )
            .unwrap();
        }

        let metrics = compute_evidence_metrics(workspace).expect("should compute metrics");
        assert_eq!(metrics.subtask_total, 1);
        assert_eq!(metrics.subtask_done, 1);
        assert_eq!(metrics.subtask_failed, 0);
        assert!((metrics.completion_ratio - 1.0).abs() < f64::EPSILON);
        assert_eq!(metrics.total_attempts, 2);
        assert_eq!(metrics.multi_attempt_count, 1);
        assert_eq!(metrics.review_passed, 1);
        assert_eq!(metrics.review_failed, 1);
        assert_eq!(metrics.test_passed, 2);
        assert_eq!(metrics.test_failed, 0);
        assert_eq!(metrics.qa_passed, 0);
        assert_eq!(metrics.qa_failed, 1);
        assert!(metrics.plan_completed);
        assert_eq!(metrics.total_events, 6);
        assert!(metrics.health_score > 0, "health score should be positive");
    }

    #[test]
    fn compute_metrics_returns_none_without_board() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_str().unwrap();
        // No BLACKBOARD.json, no events
        assert!(compute_evidence_metrics(workspace).is_none());
    }

    #[test]
    fn format_metrics_section_includes_key_fields() {
        let metrics = EvidenceMetrics {
            subtask_total: 5,
            subtask_done: 3,
            subtask_failed: 1,
            subtask_in_progress: 0,
            subtask_needs_fix: 1,
            subtask_pending: 0,
            completion_ratio: 0.6,
            total_attempts: 8,
            avg_attempts: 1.6,
            multi_attempt_count: 2,
            total_events: 20,
            review_passed: 2,
            review_failed: 1,
            test_passed: 3,
            test_failed: 0,
            qa_passed: 1,
            qa_failed: 1,
            plan_completed: true,
            debug_sessions: 1,
            health_score: 65,
        };
        let section = format_metrics_section(&metrics);
        assert!(section.contains("Quantitative Evidence Metrics"));
        assert!(section.contains("3/5"));
        assert!(section.contains("60%"));
        assert!(section.contains("**65/100**"));
        assert!(section.contains("Health score"));
        assert!(section.contains("Scoring guidance"));
    }
}
