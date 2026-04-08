/// Skills module — each skill lives in its own subfile.
///
/// Adding a new skill:
///   1. Create src/skills/<name>.rs with a `pub(super) async fn run(...)` function
///   2. Add `mod <name>;` below
///   3. Add a match arm in `execute()`
///   4. Optionally add a new prompt file in src-tauri/prompts/
use crate::{config::AppConfig, prompts::Prompts};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub(crate) mod blackboard;
mod blackboard_parser;
mod blackboard_render;
mod code;
mod code_events;
mod code_prompts;
mod debug;
pub(crate) mod isolated_workspace;
pub(crate) mod merge_engine;
mod plan;
mod plan_board;
mod qa;
mod review;
mod runner_claude;
mod runner_codex;
mod runner_lifecycle;
mod runner_process;
mod runner_workspace;
pub(crate) mod runners;
pub(crate) mod test_skill;
mod vendored;

// ── Shared event payload types ─────────────────────────────────────────────────

/// Streamed token chunk sent to the frontend via the "skill-chunk" Tauri event.
#[derive(Serialize, Clone)]
pub struct SkillChunk {
    pub agent: String,
    pub text: String,
    /// true = frontend should start a new message bubble for this agent
    pub reset: bool,
}

/// Outcome of a single review phase, emitted via "review-phase-result".
#[derive(Serialize, Clone)]
pub struct ReviewPhaseResult {
    pub phase: String,
    pub passed: bool,
    pub issue: String,
}

/// Outcome of a QA acceptance pass, emitted via "qa-result".
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QaResult {
    pub verdict: String,
    pub recommended_next_step: String,
    pub summary: String,
    pub issue: String,
    /// Quantitative confidence score (0-100) from the QA agent.
    #[serde(default)]
    pub confidence_score: u32,
    /// Pre-computed health score from evidence metrics (0-100).
    #[serde(default)]
    pub health_score: u32,
}

/// Tool-call entry emitted via "tool-log" whenever Claude or Codex calls a tool.
#[derive(Serialize, Clone)]
pub struct ToolLog {
    pub agent: String,
    pub tool: String,
    pub input: String,
    pub timestamp: u64,
}

/// Shared-blackboard update emitted by code mode when a subtask advances.
#[derive(Serialize, Deserialize, Clone)]
pub struct BlackboardEvent {
    pub subtask_id: Option<String>,
    pub status: String,
    pub summary: String,
}

pub(crate) fn sanitize_blackboard_state(workspace: &str) -> Result<(), String> {
    blackboard::sanitize_persisted_state(workspace)
}

// ── Context injection helper (used by skill submodules) ───────────────────────

/// Prepend the project context document to a prompt, if one was supplied.
pub(super) fn inject_context(context: Option<&str>, prompt: String) -> String {
    match context {
        Some(ctx) if !ctx.trim().is_empty() => {
            format!("## Project Context\n\n{ctx}\n\n---\n\n{prompt}")
        }
        _ => prompt,
    }
}

pub(super) fn merge_context_sections(sections: &[Option<String>]) -> Option<String> {
    let merged = sections
        .iter()
        .filter_map(|section| section.as_deref())
        .map(str::trim)
        .filter(|section| !section.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if merged.is_empty() {
        None
    } else {
        Some(merged.join("\n\n---\n\n"))
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Execute the named skill for the given task.
/// Streams output to the frontend via Tauri events scoped to the given window.
pub async fn execute(
    mode: &str,
    task: &str,
    workspace: Option<&str>,
    phase: Option<&str>,
    context: Option<&str>,
    issue: Option<&str>,
    config: &AppConfig,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    match mode {
        "plan" => {
            plan::run(
                task,
                workspace,
                context,
                prompts,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        "code" => {
            code::run(
                task,
                workspace,
                context,
                config,
                prompts,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        "debug" => {
            debug::run(
                task,
                workspace,
                context,
                config,
                prompts,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        "test" => {
            test_skill::run_phase(
                phase.unwrap_or("integration_test"),
                task,
                issue,
                workspace,
                context,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        "review" => {
            review::run_phase(
                phase.unwrap_or("plan_check"),
                task,
                issue,
                workspace,
                context,
                Some(prompts),
                window_label,
                app_handle,
                token,
            )
            .await
        }
        "qa" => {
            qa::run(
                task,
                issue,
                workspace,
                context,
                prompts,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        other => {
            tracing::error!(mode = other, "unknown skill requested");
            Err(format!("Unknown skill: {other}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── inject_context ────────────────────────────────────────────────────────

    #[test]
    fn inject_context_prepends_when_present() {
        let result = inject_context(Some("doc content"), "base prompt".to_string());
        assert!(result.starts_with("## Project Context\n\ndoc content\n\n---\n\n"));
        assert!(result.ends_with("base prompt"));
    }

    #[test]
    fn inject_context_noop_when_none() {
        let result = inject_context(None, "base prompt".to_string());
        assert_eq!(result, "base prompt");
    }

    #[test]
    fn inject_context_noop_when_whitespace_only() {
        let result = inject_context(Some("   \n  "), "base prompt".to_string());
        assert_eq!(result, "base prompt");
    }

    #[test]
    fn inject_context_empty_string_noop() {
        let result = inject_context(Some(""), "base".to_string());
        assert_eq!(result, "base");
    }

    #[test]
    fn merge_context_sections_skips_empty_entries() {
        let merged = merge_context_sections(&[
            Some("alpha".to_string()),
            None,
            Some(" \n ".to_string()),
            Some("beta".to_string()),
        ])
        .unwrap();
        assert_eq!(merged, "alpha\n\n---\n\nbeta");
    }

    #[test]
    fn merge_context_sections_returns_none_when_empty() {
        let merged = merge_context_sections(&[None, Some("   ".to_string())]);
        assert!(merged.is_none());
    }

    // ── execute dispatch ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_unknown_mode_returns_error() {
        let result: Result<(), String> = Err(format!("Unknown skill: {}", "fly"));
        assert_eq!(result.unwrap_err(), "Unknown skill: fly");
    }
}
