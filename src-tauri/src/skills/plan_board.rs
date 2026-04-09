use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const PLAN_BOARD_JSON: &str = ".ai-dev-hub/PLAN_BLACKBOARD.json";
pub(crate) const PLAN_BOARD_MD: &str = ".ai-dev-hub/PLAN_BLACKBOARD.md";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanBoardMode {
    Scratch,
    Review,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanBoardState {
    Pending,
    InProgress,
    Completed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanBoard {
    pub task: String,
    pub mode: PlanBoardMode,
    pub state: PlanBoardState,
    pub source_document_present: bool,
    pub claude_round_1: Option<String>,
    pub codex_round_2: Option<String>,
    pub claude_round_3: Option<String>,
    pub codex_round_4: Option<String>,
    pub updated_at: String,
}

impl PlanBoard {
    pub(crate) fn new(task: &str, mode: PlanBoardMode, source_document_present: bool) -> Self {
        Self {
            task: task.to_string(),
            mode,
            state: PlanBoardState::Pending,
            source_document_present,
            claude_round_1: None,
            codex_round_2: None,
            claude_round_3: None,
            codex_round_4: None,
            updated_at: now_string(),
        }
    }

    pub(crate) fn persist(&self, workspace: &str) -> Result<(), String> {
        let root = Path::new(workspace);
        let json_path = root.join(PLAN_BOARD_JSON);
        let md_path = root.join(PLAN_BOARD_MD);
        if let Some(parent) = json_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Cannot serialize plan blackboard: {e}"))?;
        // Atomic write: write to temp then rename to prevent partial reads.
        atomic_write(&json_path, json.as_bytes())?;
        // Markdown is a derived rendering — its failure must not kill the
        // planning phase since the authoritative JSON was already written.
        if let Err(e) = atomic_write(&md_path, self.render_markdown().as_bytes()) {
            tracing::warn!("Failed to write {} (non-fatal): {e}", md_path.display());
        }
        Ok(())
    }

    pub(crate) fn set_round_1(&mut self, text: String) {
        self.state = PlanBoardState::InProgress;
        self.claude_round_1 = Some(text);
        self.updated_at = now_string();
    }

    pub(crate) fn set_round_2(&mut self, text: String) {
        self.state = PlanBoardState::InProgress;
        self.codex_round_2 = Some(text);
        self.updated_at = now_string();
    }

    pub(crate) fn set_round_3(&mut self, text: String) {
        self.state = PlanBoardState::InProgress;
        self.claude_round_3 = Some(text);
        self.updated_at = now_string();
    }

    pub(crate) fn set_round_4(&mut self, text: String) {
        self.codex_round_4 = Some(text);
        self.state = PlanBoardState::Completed;
        self.updated_at = now_string();
    }

    pub(crate) fn render_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Plan Shared Blackboard\n\n");
        out.push_str("This board is the single shared state for plan-mode collaboration.\n");
        out.push_str("Agents must read this board instead of relying on direct agent-to-agent transcript passing.\n\n");
        out.push_str("## Meta\n\n");
        out.push_str(&format!("- Task: {}\n", self.task));
        out.push_str(&format!("- Mode: {}\n", mode_label(&self.mode)));
        out.push_str(&format!("- State: {}\n", state_label(&self.state)));
        out.push_str(&format!(
            "- Source document provided: {}\n",
            if self.source_document_present {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!("- Last updated: {}\n\n", self.updated_at));

        out.push_str("## Round 1 - Claude\n\n");
        out.push_str(self.claude_round_1.as_deref().unwrap_or("_pending_"));
        out.push_str("\n\n## Round 2 - Codex\n\n");
        out.push_str(self.codex_round_2.as_deref().unwrap_or("_pending_"));
        out.push_str("\n\n## Round 3 - Claude\n\n");
        out.push_str(self.claude_round_3.as_deref().unwrap_or("_pending_"));
        out.push_str("\n\n## Round 4 - Codex\n\n");
        out.push_str(self.codex_round_4.as_deref().unwrap_or("_pending_"));
        out.push('\n');
        out
    }
}

/// Write to a temp file then rename for crash safety.
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    // Include original extension in temp name to avoid collision when
    // persisting .json and .md files with the same stem sequentially.
    let ext = path
        .extension()
        .map(|e| format!("{}.tmp", e.to_string_lossy()))
        .unwrap_or_else(|| "tmp".to_string());
    let tmp = path.with_extension(ext);
    std::fs::write(&tmp, data)
        .map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("Cannot rename {} -> {}: {e}", tmp.display(), path.display()))
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn mode_label(mode: &PlanBoardMode) -> &'static str {
    match mode {
        PlanBoardMode::Scratch => "scratch",
        PlanBoardMode::Review => "review",
    }
}

fn state_label(state: &PlanBoardState) -> &'static str {
    match state {
        PlanBoardState::Pending => "pending",
        PlanBoardState::InProgress => "in_progress",
        PlanBoardState::Completed => "completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_mentions_shared_state_contract() {
        let board = PlanBoard::new("demo", PlanBoardMode::Scratch, false);
        let text = board.render_markdown();
        assert!(text.contains("single shared state"));
        assert!(text.contains("Round 1 - Claude"));
    }
}
