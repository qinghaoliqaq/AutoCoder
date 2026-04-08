use super::{
    emit_skill_event,
    plan_board::{PlanBoard, PlanBoardMode, PLAN_BOARD_MD},
    record_skill_evidence, runners,
};
/// Plan skill — shared-blackboard orchestration for both scratch planning and
/// document-review planning.
use crate::{
    planning_schema::{
        parse_plan_acceptance, parse_plan_graph, validate_acceptance_matches_graph,
        validate_plan_quality, PLAN_ACCEPTANCE_JSON, PLAN_GRAPH_JSON,
    },
    prompts::Prompts,
};
use dirs;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task: &str,
    _workspace: Option<&str>,
    context: Option<&str>,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let fallback_name = task_to_dirname(task);
    let naming_prompt = Prompts::render(&prompts.plan_name, &[("task", task)]);
    let (base_name, naming_fallback_reason) = match runners::claude_read_only_quiet(
        &naming_prompt,
        None,
        window_label,
        app_handle,
        token.clone(),
    )
    .await
    {
        Ok(name_output) => match extract_project_dir(&name_output) {
            Some(name) => (name, None),
            None => (
                fallback_name.clone(),
                Some(format!(
                    "Claude returned an invalid workspace name for plan naming. Using fallback '{fallback_name}'."
                )),
            ),
        },
        Err(err) => (
            fallback_name.clone(),
            Some(format!(
                "Claude plan naming failed. Using fallback '{fallback_name}'. Cause: {err}"
            )),
        ),
    };

    if let Some(reason) = naming_fallback_reason {
        emit_skill_event(app_handle, window_label, "plan_name_fallback", reason)?;
    }

    let ws_path = create_plan_workspace_unique(&base_name)?;
    let ws_str = ws_path.to_string_lossy().into_owned();
    let plan_path_str = ws_path.join("PLAN.md").to_string_lossy().into_owned();
    let plan_graph_path_str = ws_path.join(PLAN_GRAPH_JSON).to_string_lossy().into_owned();
    let plan_acceptance_path_str = ws_path
        .join(PLAN_ACCEPTANCE_JSON)
        .to_string_lossy()
        .into_owned();

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "plan-workspace",
            &ws_str,
        )
        .map_err(|e| format!("Emit error: {e}"))?;
    record_skill_evidence(
        Some(&ws_str),
        "plan_started",
        &format!("Planning started for task: {task}"),
        "system",
        vec!["PLAN.md".to_string()],
    );

    if let Some(doc) = context.filter(|c| !c.trim().is_empty()) {
        run_review_mode(
            task,
            doc,
            &plan_path_str,
            &plan_graph_path_str,
            &plan_acceptance_path_str,
            &ws_str,
            prompts,
            window_label,
            app_handle,
            token.clone(),
        )
        .await?;
    } else {
        run_scratch_mode(
            task,
            &plan_path_str,
            &plan_graph_path_str,
            &plan_acceptance_path_str,
            &ws_str,
            prompts,
            window_label,
            app_handle,
            token.clone(),
        )
        .await?;
    }

    let plan_doc =
        validate_or_repair_plan_artifacts(task, &ws_path, window_label, app_handle, token).await?;

    record_skill_evidence(
        Some(&ws_str),
        "plan_completed",
        &format!(
            "Planning completed. PLAN.md, {PLAN_GRAPH_JSON}, and {PLAN_ACCEPTANCE_JSON} validated."
        ),
        "system",
        vec![
            "PLAN.md".to_string(),
            PLAN_GRAPH_JSON.to_string(),
            PLAN_ACCEPTANCE_JSON.to_string(),
            PLAN_BOARD_MD.to_string(),
        ],
    );

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
    task: &str,
    plan_path: &str,
    plan_graph_path: &str,
    plan_acceptance_path: &str,
    ws_dir: &str,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    let mut board = PlanBoard::new(task, PlanBoardMode::Scratch, false);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "initialized",
        "Plan blackboard initialized for scratch planning.".to_string(),
    )?;

    let r1 = Prompts::render(
        &prompts.plan_claude,
        &[("task", task), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let proposals = runners::claude(&r1, Some(ws_dir), window_label, app_handle, token.clone())
        .await
        .map_err(|err| stage_error("scratch_round_1_claude", "claude", Some(ws_dir), &r1, err))?;
    board.set_round_1(proposals);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_1",
        "Claude recorded proposal candidates on the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_round_1",
        "Claude recorded proposal candidates on the shared plan blackboard.",
        "claude",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r2 = Prompts::render(
        &prompts.plan_codex,
        &[("task", task), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let evaluation =
        runners::codex_read_only(&r2, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error(
                    "scratch_round_2_codex",
                    "codex_read_only",
                    Some(ws_dir),
                    &r2,
                    err,
                )
            })?;
    board.set_round_2(evaluation);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_2",
        "Codex evaluated the proposals by reading the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_round_2",
        "Codex evaluated the proposals by reading the shared plan blackboard.",
        "codex",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r3 = Prompts::render(
        &prompts.plan_claude_response,
        &[("task", task), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let claude_rebuttal =
        runners::claude(&r3, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error("scratch_round_3_claude", "claude", Some(ws_dir), &r3, err)
            })?;
    board.set_round_3(claude_rebuttal);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_3",
        "Claude updated the shared plan blackboard with rebuttals and refinements.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_round_3",
        "Claude updated the shared plan blackboard with rebuttals and refinements.",
        "claude",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r4 = Prompts::render(
        &prompts.plan_codex_final,
        &[("task", task), ("plan_board_path", PLAN_BOARD_MD)],
    );
    let verdict =
        runners::codex_read_only(&r4, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error(
                    "scratch_round_4_codex",
                    "codex_read_only",
                    Some(ws_dir),
                    &r4,
                    err,
                )
            })?;
    board.set_round_4(verdict);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_4",
        "Codex wrote the final planning verdict to the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_round_4",
        "Codex wrote the final planning verdict to the shared plan blackboard.",
        "codex",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r5 = Prompts::render(
        &prompts.plan_synthesis,
        &[
            ("task", task),
            ("plan_path", plan_path),
            ("plan_graph_path", plan_graph_path),
            ("plan_acceptance_path", plan_acceptance_path),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    runners::claude(&r5, Some(ws_dir), window_label, app_handle, token.clone())
        .await
        .map_err(|err| stage_error("scratch_synthesis_claude", "claude", Some(ws_dir), &r5, err))
}

async fn run_review_mode(
    task: &str,
    document: &str,
    plan_path: &str,
    plan_graph_path: &str,
    plan_acceptance_path: &str,
    ws_dir: &str,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    let mut board = PlanBoard::new(task, PlanBoardMode::Review, true);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "initialized",
        "Plan blackboard initialized for document review.".to_string(),
    )?;

    let r1 = Prompts::render(
        &prompts.plan_review_claude,
        &[
            ("task", task),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    let claude_analysis =
        runners::claude(&r1, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error("review_round_1_claude", "claude", Some(ws_dir), &r1, err)
            })?;
    board.set_round_1(claude_analysis);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_1",
        "Claude wrote the initial document analysis onto the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_review_round_1",
        "Claude wrote the initial document analysis onto the shared plan blackboard.",
        "claude",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r2 = Prompts::render(
        &prompts.plan_review_codex,
        &[
            ("task", task),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    let codex_analysis =
        runners::codex_read_only(&r2, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error(
                    "review_round_2_codex",
                    "codex_read_only",
                    Some(ws_dir),
                    &r2,
                    err,
                )
            })?;
    board.set_round_2(codex_analysis);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_2",
        "Codex added its review perspective via the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_review_round_2",
        "Codex added its review perspective via the shared plan blackboard.",
        "codex",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r3 = Prompts::render(
        &prompts.plan_review_claude_resp,
        &[
            ("task", task),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    let change_list = runners::claude(&r3, Some(ws_dir), window_label, app_handle, token.clone())
        .await
        .map_err(|err| stage_error("review_round_3_claude", "claude", Some(ws_dir), &r3, err))?;
    board.set_round_3(change_list);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_3",
        "Claude consolidated the change list on the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_review_round_3",
        "Claude consolidated the change list on the shared plan blackboard.",
        "claude",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r4 = Prompts::render(
        &prompts.plan_review_codex_final,
        &[
            ("task", task),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    let final_changes =
        runners::codex_read_only(&r4, Some(ws_dir), window_label, app_handle, token.clone())
            .await
            .map_err(|err| {
                stage_error(
                    "review_round_4_codex",
                    "codex_read_only",
                    Some(ws_dir),
                    &r4,
                    err,
                )
            })?;
    board.set_round_4(final_changes);
    board.persist(ws_dir)?;
    emit_skill_event(
        app_handle,
        window_label,
        "round_4",
        "Codex finalized the approved changes on the shared plan blackboard.".to_string(),
    )?;
    record_skill_evidence(
        Some(ws_dir),
        "plan_review_round_4",
        "Codex finalized the approved changes on the shared plan blackboard.",
        "codex",
        vec![PLAN_BOARD_MD.to_string()],
    );

    let r5 = Prompts::render(
        &prompts.plan_review_synthesis,
        &[
            ("task", task),
            ("plan_path", plan_path),
            ("plan_graph_path", plan_graph_path),
            ("plan_acceptance_path", plan_acceptance_path),
            ("document", document),
            ("plan_board_path", PLAN_BOARD_MD),
        ],
    );
    runners::claude(&r5, Some(ws_dir), window_label, app_handle, token.clone())
        .await
        .map_err(|err| stage_error("review_synthesis_claude", "claude", Some(ws_dir), &r5, err))
}

async fn validate_or_repair_plan_artifacts(
    task: &str,
    workspace: &std::path::Path,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    let validated = match validate_plan_artifacts(workspace) {
        Ok(v) => v,
        Err(validation_err) => {
            emit_skill_event(
                app_handle,
                window_label,
                "structured_plan_repair",
                format!(
                    "PLAN.md was written, but the structured planning artifacts were invalid. Claude is repairing {} and {}.",
                    PLAN_GRAPH_JSON, PLAN_ACCEPTANCE_JSON
                ),
            )?;

            let repair_prompt = format!(
                "You just finished planning for task: {task}\n\n\
                 PLAN.md already exists in the current directory and must remain the source of truth.\n\
                 Read these files before making changes:\n\
                 - PLAN.md\n\
                 - {PLAN_BOARD_MD}\n\n\
                 Fix ONLY these structured files in the current directory:\n\
                 - {PLAN_GRAPH_JSON}\n\
                 - {PLAN_ACCEPTANCE_JSON}\n\n\
                 Validation failed with this error:\n\
                 {validation_err}\n\n\
                 Requirements:\n\
                 - The JSON must match PLAN.md exactly.\n\
                 - {PLAN_GRAPH_JSON} must include every planned subtask with valid depends_on references.\n\
                 - {PLAN_ACCEPTANCE_JSON} must include one acceptance entry for every subtask in {PLAN_GRAPH_JSON}.\n\
                 - Do not modify PLAN.md unless it is absolutely required to restore consistency, and if you do, keep the scope minimal.\n\
                 - Output valid JSON only in the files, no comments.\n\
                 At the very end, output exactly one line and nothing else:\n\
                 PLAN_ARTIFACTS_FIXED"
            );
            runners::claude(
                &repair_prompt,
                Some(workspace.to_string_lossy().as_ref()),
                window_label,
                app_handle,
                token,
            )
            .await
            .map_err(|err| {
                stage_error(
                    "structured_plan_repair_claude",
                    "claude",
                    Some(workspace.to_string_lossy().as_ref()),
                    &repair_prompt,
                    err,
                )
            })?;
            validate_plan_artifacts(workspace)?
        }
    };

    // Emit quality warnings (advisory, non-blocking).
    for warning in &validated.quality_warnings {
        emit_skill_event(
            app_handle,
            window_label,
            "plan_quality_warning",
            warning.clone(),
        )?;
    }
    if !validated.quality_warnings.is_empty() {
        record_skill_evidence(
            Some(workspace.to_string_lossy().as_ref()),
            "plan_quality_warnings",
            &format!(
                "{} plan quality warning(s) detected.",
                validated.quality_warnings.len()
            ),
            "system",
            validated.quality_warnings,
        );
    }

    Ok(validated.plan_doc)
}

fn stage_error(
    stage: &str,
    agent: &str,
    workspace: Option<&str>,
    prompt: &str,
    err: String,
) -> String {
    let workspace = workspace.unwrap_or("<pending>");
    let prompt_chars = prompt.chars().count();
    let prompt_lines = prompt.lines().count();
    format!(
        "Plan stage '{stage}' failed (agent={agent}, workspace={workspace}, prompt_chars={prompt_chars}, prompt_lines={prompt_lines}): {err}"
    )
}

/// Validated plan output — the plan document plus any quality warnings.
struct ValidatedPlan {
    plan_doc: String,
    quality_warnings: Vec<String>,
}

fn validate_plan_artifacts(workspace: &std::path::Path) -> Result<ValidatedPlan, String> {
    let plan_path = workspace.join("PLAN.md");
    let graph_path = workspace.join(PLAN_GRAPH_JSON);
    let acceptance_path = workspace.join(PLAN_ACCEPTANCE_JSON);

    let plan_doc = std::fs::read_to_string(&plan_path)
        .map_err(|e| format!("Cannot read {} after synthesis: {e}", plan_path.display()))?;
    let graph_doc = std::fs::read_to_string(&graph_path)
        .map_err(|e| format!("Cannot read {} after synthesis: {e}", graph_path.display()))?;
    let acceptance_doc = std::fs::read_to_string(&acceptance_path).map_err(|e| {
        format!(
            "Cannot read {} after synthesis: {e}",
            acceptance_path.display()
        )
    })?;

    let graph = parse_plan_graph(&graph_doc)?;
    let acceptance = parse_plan_acceptance(&acceptance_doc)?;
    validate_acceptance_matches_graph(&graph, &acceptance)?;

    let quality_warnings = validate_plan_quality(&graph, &acceptance);

    Ok(ValidatedPlan {
        plan_doc,
        quality_warnings,
    })
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

    Err(format!(
        "Cannot find a unique workspace name for '{base_name}' (tried up to -99)"
    ))
}

fn extract_project_dir(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("PROJECT_DIR:") {
            if let Some(clean) = normalize_project_dir_candidate(rest) {
                return Some(clean);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("project_dir:") {
            if let Some(clean) = normalize_project_dir_candidate(rest) {
                return Some(clean);
            }
            continue;
        }

        if let Some(clean) = normalize_project_dir_candidate(trimmed) {
            return Some(clean);
        }
    }
    None
}

fn normalize_project_dir_candidate(candidate: &str) -> Option<String> {
    let stripped = candidate
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'');
    if stripped.is_empty() {
        return None;
    }

    let simple_candidate = stripped
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '));
    if !simple_candidate {
        return None;
    }

    let clean: String = stripped
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let clean: String = clean.chars().take(48).collect();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
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
        format!("plan-{:08x}", stable_task_hash(task))
    } else {
        slug
    }
}

fn stable_task_hash(task: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in task.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{extract_project_dir, task_to_dirname};

    #[test]
    fn extract_project_dir_parses_valid_output() {
        assert_eq!(
            extract_project_dir("PROJECT_DIR: smart-recruitment"),
            Some("smart-recruitment".to_string())
        );
    }

    #[test]
    fn extract_project_dir_normalizes_invalid_chars() {
        assert_eq!(
            extract_project_dir("PROJECT_DIR: Smart Recruitment_System"),
            Some("smart-recruitment-system".to_string())
        );
    }

    #[test]
    fn extract_project_dir_accepts_plain_single_line_slug() {
        assert_eq!(extract_project_dir("api-2"), Some("api-2".to_string()));
    }

    #[test]
    fn extract_project_dir_accepts_quoted_slug() {
        assert_eq!(extract_project_dir("`api-2`"), Some("api-2".to_string()));
    }

    #[test]
    fn task_to_dirname_hashes_non_ascii_only_tasks() {
        let name = task_to_dirname("智能招聘系统");
        assert!(name.starts_with("plan-"));
        assert_eq!(name.len(), 13);
    }
}
