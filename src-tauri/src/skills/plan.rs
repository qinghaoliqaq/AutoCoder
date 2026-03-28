/// Plan skill — shared-blackboard orchestration for both scratch planning and
/// document-review planning.

use crate::prompts::Prompts;
use dirs;
use super::{
    plan_board::{PlanBoard, PlanBoardMode, PLAN_BOARD_MD},
    runners, BlackboardEvent,
};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task:         &str,
    _workspace:   Option<&str>,
    context:      Option<&str>,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    let name_prompt = Prompts::render(&prompts.plan_name, &[("task", task)]);
    let name_output = runners::claude_silent(&name_prompt, None).await?;
    let base_name = extract_project_dir(&name_output).unwrap_or_else(|| task_to_dirname(task));

    let ws_path = create_plan_workspace_unique(&base_name)?;
    let ws_str = ws_path.to_string_lossy().into_owned();
    let plan_path_str = ws_path.join("PLAN.md").to_string_lossy().into_owned();

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "plan-workspace",
            &ws_str,
        )
        .map_err(|e| format!("Emit error: {e}"))?;

    if let Some(doc) = context.filter(|c| !c.trim().is_empty()) {
        run_review_mode(task, doc, &plan_path_str, &ws_str, prompts, window_label, app_handle, token).await?;
    } else {
        run_scratch_mode(task, &plan_path_str, &ws_str, prompts, window_label, app_handle, token).await?;
    }

    let plan_doc = std::fs::read_to_string(ws_path.join("PLAN.md"))
        .map_err(|e| format!("Cannot read PLAN.md after synthesis: {e}"))?;

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "plan-report",
            &plan_doc,
        )
        .map_err(|e| format!("Emit error: {e}"))?;

    Ok(())
}

async fn run_scratch_mode(
    task:         &str,
    plan_path:    &str,
    ws_dir:       &str,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let mut board = PlanBoard::new(task, PlanBoardMode::Scratch, false);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "initialized",
        "Plan blackboard initialized for scratch planning.".to_string(),
    )?;

    let r1 = Prompts::render(&prompts.plan_claude, &[("task", task), ("plan_board_path", PLAN_BOARD_MD)]);
    let proposals = runners::claude(&r1, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_1(proposals);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_1",
        "Claude recorded proposal candidates on the shared plan blackboard.".to_string(),
    )?;

    let r2 = Prompts::render(&prompts.plan_codex, &[("task", task), ("plan_board_path", PLAN_BOARD_MD)]);
    let evaluation = runners::codex(&r2, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_2(evaluation);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_2",
        "Codex evaluated the proposals by reading the shared plan blackboard.".to_string(),
    )?;

    let r3 = Prompts::render(&prompts.plan_claude_response, &[("task", task), ("plan_board_path", PLAN_BOARD_MD)]);
    let claude_rebuttal = runners::claude(&r3, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_3(claude_rebuttal);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_3",
        "Claude updated the shared plan blackboard with rebuttals and refinements.".to_string(),
    )?;

    let r4 = Prompts::render(&prompts.plan_codex_final, &[("task", task), ("plan_board_path", PLAN_BOARD_MD)]);
    let verdict = runners::codex(&r4, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_4(verdict);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_4",
        "Codex wrote the final planning verdict to the shared plan blackboard.".to_string(),
    )?;

    let r5 = Prompts::render(
        &prompts.plan_synthesis,
        &[("task", task), ("plan_path", plan_path), ("plan_board_path", PLAN_BOARD_MD)],
    );
    runners::claude(&r5, Some(ws_dir), window_label, app_handle, token.clone()).await
}

async fn run_review_mode(
    task:         &str,
    document:     &str,
    plan_path:    &str,
    ws_dir:       &str,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let mut board = PlanBoard::new(task, PlanBoardMode::Review, true);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "initialized",
        "Plan blackboard initialized for document review.".to_string(),
    )?;

    let r1 = Prompts::render(
        &prompts.plan_review_claude,
        &[("task", task), ("document", document), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let claude_analysis = runners::claude(&r1, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_1(claude_analysis);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_1",
        "Claude wrote the initial document analysis onto the shared plan blackboard.".to_string(),
    )?;

    let r2 = Prompts::render(
        &prompts.plan_review_codex,
        &[("task", task), ("document", document), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let codex_analysis = runners::codex(&r2, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_2(codex_analysis);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_2",
        "Codex added its review perspective via the shared plan blackboard.".to_string(),
    )?;

    let r3 = Prompts::render(
        &prompts.plan_review_claude_resp,
        &[("task", task), ("document", document), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let change_list = runners::claude(&r3, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_3(change_list);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_3",
        "Claude consolidated the change list on the shared plan blackboard.".to_string(),
    )?;

    let r4 = Prompts::render(
        &prompts.plan_review_codex_final,
        &[("task", task), ("document", document), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let final_changes = runners::codex(&r4, Some(ws_dir), window_label, app_handle, token.clone()).await?;
    board.set_round_4(final_changes);
    board.persist(ws_dir)?;
    emit_plan_event(
        app_handle,
        window_label,
        "round_4",
        "Codex finalized the approved changes on the shared plan blackboard.".to_string(),
    )?;

    let r5 = Prompts::render(
        &prompts.plan_review_synthesis,
        &[
            ("task", task),
            ("plan_path", plan_path),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    runners::claude(&r5, Some(ws_dir), window_label, app_handle, token.clone()).await
}

fn emit_plan_event(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    status: &str,
    summary: String,
) -> Result<(), String> {
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "blackboard-updated",
            BlackboardEvent {
                subtask_id: None,
                status: status.to_string(),
                summary,
            },
        )
        .map_err(|e| format!("Emit error: {e}"))
}

fn extract_project_dir(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("PROJECT_DIR:") {
            let name = rest.trim().to_lowercase();
            let clean: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            let clean: String = clean.chars().take(48).collect();
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    None
}

fn create_plan_workspace_unique(base_name: &str) -> Result<std::path::PathBuf, String> {
    let desktop = dirs::desktop_dir().ok_or("Cannot locate Desktop directory")?;
    let candidate = desktop.join(base_name);
    if !candidate.exists() {
        std::fs::create_dir_all(&candidate)
            .map_err(|e| format!("Cannot create workspace '{base_name}': {e}"))?;
        return Ok(candidate);
    }

    for n in 2u32..=99 {
        let name = format!("{base_name}-{n}");
        let candidate = desktop.join(&name);
        if !candidate.exists() {
            std::fs::create_dir_all(&candidate)
                .map_err(|e| format!("Cannot create workspace '{name}': {e}"))?;
            return Ok(candidate);
        }
    }

    Err(format!("Cannot find a unique workspace name for '{base_name}' (tried up to -99)"))
}

fn task_to_dirname(task: &str) -> String {
    let slug: String = task
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    if slug.is_empty() {
        "plan-draft".to_string()
    } else {
        slug
    }
}
