/// Code skill — per-subtask implementation loop driven by a shared blackboard.
///
/// Orchestrates: scheduling → isolated workspace → Claude implement → verifier
/// → Codex review → three-way merge back to main workspace.
///
/// Heavy-lifting is delegated to submodules:
///   isolated_workspace — fork / sync / cleanup / snapshot / diff
///   merge_engine       — three-way line-level merge
///   code_prompts       — prompt builders and output parsers
///   code_events        — Tauri event emission and evidence recording

use super::{
    blackboard::{tick_plan_checkbox, Blackboard, SubtaskCard, BLACKBOARD_MD},
    code_events::{emit_blackboard, emit_vendored_skill_log},
    code_prompts::{
        build_fix_prompt, build_implement_prompt, build_review_prompt,
        parse_implementation_report, parse_review_report,
    },
    isolated_workspace::{
        cleanup_isolated_workspace, create_isolated_workspace, relative_paths_from_root,
        snapshot_workspace, sync_coordination_files, workspace_changes,
    },
    merge_engine::merge_isolated_workspace,
    runners,
    vendored::{load as load_vendored_skill, select_for_subtask},
};
use crate::{
    config::AppConfig,
    evidence,
    planning_schema::{read_plan_acceptance_lenient, SubtaskAcceptance},
    prompts::Prompts,
    verifier,
};
use std::collections::{HashMap, HashSet};
use tokio::{sync::Mutex, task::JoinSet};
use tokio_util::sync::CancellationToken;

const MAX_SUBTASK_ATTEMPTS: u32 = 3;

pub(super) async fn run(
    task: &str,
    workspace: Option<&str>,
    context: Option<&str>,
    config: &AppConfig,
    prompts: &Prompts,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let workspace = workspace.ok_or("Code mode requires an existing workspace from plan mode")?;
    let board = Blackboard::load_or_create(workspace, task)?;
    let parallel_limit = config.features.parallel_subtask_limit();
    let total = board.subtasks.len();
    board.persist(workspace)?;
    let (acceptance, acceptance_warning) = read_plan_acceptance_lenient(workspace);
    let acceptance_by_subtask = std::sync::Arc::new(
        acceptance
            .map(|acceptance| {
                acceptance
                    .subtasks
                    .into_iter()
                    .map(|subtask| (subtask.subtask_id.clone(), subtask))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default(),
    );

    emit_blackboard(
        workspace,
        app_handle,
        window_label,
        None,
        "initialized",
        format!(
            "Shared blackboard initialized. {} subtasks loaded from PLAN.md / PLAN_GRAPH.json. Parallel lanes: {}.",
            total,
            parallel_limit
        ),
    )?;
    if let Some(warning) = acceptance_warning {
        emit_blackboard(
            workspace,
            app_handle,
            window_label,
            None,
            "acceptance_unavailable",
            format!(
                "Structured acceptance data is unavailable, so code mode is falling back to PLAN.md and blackboard-only review: {warning}"
            ),
        )?;
    }

    let base_prompt = Prompts::render(&prompts.code_claude, &[("task", task)]);
    let shared_board = std::sync::Arc::new(Mutex::new(board));
    let merge_lock = std::sync::Arc::new(Mutex::new(()));

    if total == 0 {
        let mut board = shared_board.lock().await;
        board.complete_if_finished();
        board.persist(workspace)?;
        return Ok(());
    }

    let worker_limit = parallel_limit.max(1);
    let mut join_set = JoinSet::new();
    let mut launched_ids = HashSet::new();
    let mut active_subtasks = HashMap::<String, ActiveSubtaskMeta>::new();

    let fatal_error = loop {
        spawn_ready_subtasks(
            &mut join_set,
            total,
            task,
            workspace,
            context,
            config,
            &base_prompt,
            &shared_board,
            &acceptance_by_subtask,
            &merge_lock,
            window_label,
            app_handle,
            token.clone(),
            worker_limit,
            &mut launched_ids,
            &mut active_subtasks,
        )
        .await?;

        if join_set.is_empty() {
            let board = shared_board.lock().await;
            if board
                .subtasks
                .iter()
                .all(|card| matches!(card.status, super::blackboard::SubtaskState::Done))
            {
                break None;
            }
            let blocked = board
                .subtasks
                .iter()
                .filter(|card| !matches!(card.status, super::blackboard::SubtaskState::Done))
                .map(|card| {
                    if card.depends_on.is_empty() {
                        card.id.clone()
                    } else {
                        format!("{} (waiting on: {})", card.id, card.depends_on.join(", "))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            break Some(format!(
                "No schedulable subtasks remain, but work is incomplete. Check PLAN_GRAPH.json dependencies: {blocked}"
            ));
        }

        let Some(joined) = join_set.join_next().await else {
            break None;
        };

        let (subtask_id, result) = match joined {
            Ok(result) => result,
            Err(err) => break Some(format!("Parallel subtask worker crashed: {err}")),
        };
        active_subtasks.remove(&subtask_id);

        if let Err(err) = result {
            token.cancel();
            break Some(err);
        }
    };

    while let Some(joined) = join_set.join_next().await {
        if fatal_error.is_none() {
            let (_subtask_id, result) =
                joined.map_err(|err| format!("Parallel subtask worker crashed: {err}"))?;
            result?;
        }
    }

    if let Some(err) = fatal_error {
        return Err(err);
    }

    let mut board = shared_board.lock().await;
    board.complete_if_finished();
    board.persist(workspace)?;
    emit_blackboard(
        workspace,
        app_handle,
        window_label,
        None,
        "completed",
        "All planned subtasks passed inline review and were merged from isolated workspaces."
            .to_string(),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn spawn_ready_subtasks(
    join_set: &mut JoinSet<(String, Result<(), String>)>,
    total: usize,
    task: &str,
    workspace: &str,
    context: Option<&str>,
    config: &AppConfig,
    base_prompt: &str,
    board: &std::sync::Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: &std::sync::Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: &std::sync::Arc<Mutex<()>>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    worker_limit: usize,
    launched_ids: &mut HashSet<String>,
    active_subtasks: &mut HashMap<String, ActiveSubtaskMeta>,
) -> Result<(), String> {
    if active_subtasks.len() >= worker_limit {
        return Ok(());
    }

    let ready_subtasks = {
        let board = board.lock().await;
        board.schedulable_subtasks()
    };

    for card in ready_subtasks {
        if active_subtasks.len() >= worker_limit {
            break;
        }
        if launched_ids.contains(&card.id) {
            continue;
        }
        if !can_spawn_subtask(&card, active_subtasks) {
            continue;
        }

        launched_ids.insert(card.id.clone());
        active_subtasks.insert(
            card.id.clone(),
            ActiveSubtaskMeta {
                can_run_in_parallel: card.can_run_in_parallel,
                parallel_group: card.parallel_group.clone(),
            },
        );
        spawn_subtask_worker(
            join_set,
            total,
            card,
            task.to_string(),
            workspace.to_string(),
            context.map(ToOwned::to_owned),
            config.clone(),
            base_prompt.to_string(),
            board.clone(),
            acceptance_by_subtask.clone(),
            merge_lock.clone(),
            window_label.to_string(),
            app_handle.clone(),
            token.clone(),
        );
    }

    Ok(())
}

fn can_spawn_subtask(
    card: &SubtaskCard,
    active_subtasks: &HashMap<String, ActiveSubtaskMeta>,
) -> bool {
    if active_subtasks.is_empty() {
        return true;
    }
    if active_subtasks
        .values()
        .any(|active| !active.can_run_in_parallel)
    {
        return false;
    }
    if !card.can_run_in_parallel {
        return false;
    }
    if let Some(group) = &card.parallel_group {
        if active_subtasks
            .values()
            .any(|active| active.parallel_group.as_ref() == Some(group))
        {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn spawn_subtask_worker(
    join_set: &mut JoinSet<(String, Result<(), String>)>,
    total: usize,
    card: SubtaskCard,
    task: String,
    workspace: String,
    context: Option<String>,
    config: AppConfig,
    base_prompt: String,
    board: std::sync::Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: std::sync::Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: std::sync::Arc<Mutex<()>>,
    window_label: String,
    app_handle: tauri::AppHandle,
    token: CancellationToken,
) {
    join_set.spawn(async move {
        let subtask_id = card.id.clone();
        let ordinal = ordinal_for_subtask(&board, &subtask_id).await.unwrap_or(0);
        let result = run_subtask(
            ordinal,
            total,
            card,
            &task,
            &workspace,
            context.as_deref(),
            &config,
            &base_prompt,
            board,
            acceptance_by_subtask,
            merge_lock,
            &window_label,
            &app_handle,
            token,
        )
        .await;
        (subtask_id, result)
    });
}

#[allow(clippy::too_many_arguments)]
async fn run_subtask(
    ordinal: usize,
    total: usize,
    initial_card: SubtaskCard,
    task: &str,
    workspace: &str,
    context: Option<&str>,
    config: &AppConfig,
    base_prompt: &str,
    board: std::sync::Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: std::sync::Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: std::sync::Arc<Mutex<()>>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let subtask_id = initial_card.id.clone();

    loop {
        let attempt =
            mutate_board(&board, workspace, |board| board.begin_attempt(&subtask_id)).await?;

        let isolated = create_isolated_workspace(workspace, &subtask_id, attempt)?;
        mutate_board(&board, workspace, |board| {
            board.set_isolated_workspace(&subtask_id, Some(isolated.root.display().to_string()))
        })
        .await?;

        let attempt_result: Result<AttemptResolution, String> = async {
            let card = read_card(&board, &subtask_id).await?;
            let summary = if attempt == 1 {
                format!(
                    "Subtask {ordinal}/{total}: {} is now implementing {} in isolated workspace {}.",
                    card.id,
                    card.title,
                    isolated.root.display()
                )
            } else {
                format!(
                    "Subtask {ordinal}/{total}: {} needs another pass. Claude is fixing {} in isolated workspace {} using Codex findings from {}.",
                    card.id,
                    card.title,
                    isolated.root.display(),
                    BLACKBOARD_MD
                )
            };
            emit_blackboard(
                workspace,
                app_handle,
                window_label,
                Some(card.id.clone()),
                "subtask_started",
                summary,
            )?;

            let vendored_skill = match (config.features.vendored_skills, select_for_subtask(&card)) {
                (true, Some(skill_id)) => match load_vendored_skill(skill_id, app_handle) {
                    Ok(skill) => {
                        emit_vendored_skill_log(app_handle, window_label, "claude", &skill, &card)?;
                        emit_blackboard(
                            workspace,
                            app_handle,
                            window_label,
                            Some(card.id.clone()),
                            "vendored_skill_selected",
                            format!(
                                "Subtask {} is using packaged helper skill {}.",
                                card.id,
                                skill.id.label()
                            ),
                        )?;
                        Some(skill)
                    }
                    Err(err) => {
                        emit_blackboard(
                            workspace,
                            app_handle,
                            window_label,
                            Some(card.id.clone()),
                            "vendored_skill_unavailable",
                            format!(
                                "Packaged helper skill for {} is unavailable, continuing without it: {}",
                                card.id, err
                            ),
                        )?;
                        None
                    }
                },
                (false, Some(_)) => {
                    emit_blackboard(
                        workspace,
                        app_handle,
                        window_label,
                        Some(card.id.clone()),
                        "vendored_skill_disabled",
                        format!(
                            "Packaged helper skills are disabled in config. Subtask {} is continuing without them.",
                            card.id
                        ),
                    )?;
                    None
                }
                (_, None) => None,
            };

            sync_coordination_files(workspace, &isolated.root)?;
            let acceptance = acceptance_by_subtask.get(&card.id).cloned();
            let claude_prompt = if card.review_findings.is_empty() {
                build_implement_prompt(
                    base_prompt,
                    task,
                    &card,
                    acceptance.as_ref(),
                    vendored_skill.as_ref(),
                )
            } else {
                build_fix_prompt(
                    base_prompt,
                    task,
                    &card,
                    acceptance.as_ref(),
                    vendored_skill.as_ref(),
                )
            };
            let claude_prompt = super::inject_context(context, claude_prompt);
            // Inject evidence history for retries so Claude knows what failed before.
            let claude_prompt = if attempt > 1 {
                match evidence::build_subtask_context(workspace, &card.id) {
                    Some(ctx) => format!("{claude_prompt}\n\n---\n\n{ctx}"),
                    None => claude_prompt,
                }
            } else {
                claude_prompt
            };
            let claude_output = runners::claude_quiet(
                &claude_prompt,
                Some(isolated.root.to_string_lossy().as_ref()),
                window_label,
                app_handle,
                token.clone(),
            )
            .await?;

            let isolated_after = snapshot_workspace(&isolated.root);
            let isolated_changes = workspace_changes(&isolated.base_snapshot, &isolated_after);
            let observed_files = relative_paths_from_root(&isolated.root, &isolated_changes.changed_or_created);
            let implementation = parse_implementation_report(&claude_output, &observed_files, &card.id);
            mutate_board(&board, workspace, |board| {
                board.record_implementation(
                    &card.id,
                    implementation.summary.clone(),
                    implementation.files_touched.clone(),
                )
            })
            .await?;

            emit_blackboard(
                workspace,
                app_handle,
                window_label,
                Some(card.id.clone()),
                "implemented",
                format!(
                    "Claude finished {} attempt {} in isolation. Verifier is now checking the subtask.",
                    card.id, attempt
                ),
            )?;

            let verifier_result = verifier::run_and_persist(
                workspace,
                &isolated.root,
                &card,
                acceptance.as_ref(),
                &implementation.files_touched,
                &implementation.summary,
            )?;
            if verifier_result.passed {
                emit_blackboard(
                    workspace,
                    app_handle,
                    window_label,
                    Some(card.id.clone()),
                    "verifier_passed",
                    format!(
                        "Verifier passed {} attempt {}. Codex is now reviewing the subtask.",
                        card.id, attempt
                    ),
                )?;
            } else {
                mutate_board(&board, workspace, |board| {
                    board.record_review(
                        &card.id,
                        false,
                        verifier_result.summary.clone(),
                        verifier_result.findings.clone(),
                    )?;
                    board.finish_active_subtask(&card.id);
                    Ok(())
                })
                .await?;
                emit_blackboard(
                    workspace,
                    app_handle,
                    window_label,
                    Some(card.id.clone()),
                    "verifier_failed",
                    format!(
                        "Verifier blocked {} attempt {} before Codex review: {}",
                        card.id, attempt, verifier_result.summary
                    ),
                )?;

                if attempt >= MAX_SUBTASK_ATTEMPTS {
                    let reason = format!(
                        "Subtask {} failed verifier after {} attempts: {}",
                        card.id, MAX_SUBTASK_ATTEMPTS, verifier_result.summary
                    );
                    mutate_board(&board, workspace, |board| board.mark_failed(&card.id, reason.clone()))
                        .await?;
                    emit_blackboard(
                        workspace,
                        app_handle,
                        window_label,
                        Some(card.id.clone()),
                        "failed",
                        reason.clone(),
                    )?;
                    return Err(reason);
                }

                emit_blackboard(
                    workspace,
                    app_handle,
                    window_label,
                    Some(card.id.clone()),
                    "needs_fix",
                    format!(
                        "Verifier rejected {} on attempt {}. Claude will retry in a fresh isolated workspace using the verifier findings.",
                        card.id, attempt
                    ),
                )?;
                return Ok(AttemptResolution::Retry);
            }

            let review_card = read_card(&board, &card.id).await?;
            sync_coordination_files(workspace, &isolated.root)?;
            let review_prompt = super::inject_context(
                context,
                build_review_prompt(task, &review_card, acceptance.as_ref()),
            );
            let review_output = runners::codex_read_only_quiet(
                &review_prompt,
                Some(isolated.root.to_string_lossy().as_ref()),
                window_label,
                app_handle,
                token.clone(),
            )
            .await?;
            let review = parse_review_report(&review_output);

            if review.passed {
                // Serialize merges so parallel subtasks don't race on the main workspace.
                let _merge_guard = merge_lock.lock().await;
                match merge_isolated_workspace(workspace, &isolated) {
                    Ok(merged_files) => {
                        mutate_board(&board, workspace, |board| {
                            board.record_implementation(&card.id, review_card.latest_implementation.clone().unwrap_or_else(|| "Implementation merged from isolated workspace.".to_string()), merged_files.clone())?;
                            board.record_review(&card.id, true, review.summary.clone(), Vec::new())?;
                            board.finish_active_subtask(&card.id);
                            board.complete_if_finished();
                            Ok(())
                        })
                        .await?;
                        tick_plan_checkbox(workspace, &card.id)?;
                        emit_blackboard(
                            workspace,
                            app_handle,
                            window_label,
                            Some(card.id.clone()),
                            "passed",
                            format!(
                                "Subtask {} passed Codex review and merged cleanly from isolated workspace.",
                                card.id
                            ),
                        )?;
                        return Ok(AttemptResolution::Completed);
                    }
                    Err(conflict) => {
                        let mut findings = review.findings.clone();
                        findings.push(conflict.clone());

                        if attempt >= MAX_SUBTASK_ATTEMPTS {
                            let reason = format!(
                                "Subtask {} hit merge conflicts after {} attempts: {}",
                                card.id, MAX_SUBTASK_ATTEMPTS, conflict
                            );
                            mutate_board(&board, workspace, |board| {
                                board.mark_failed(&card.id, reason.clone())
                            })
                            .await?;
                            emit_blackboard(
                                workspace,
                                app_handle,
                                window_label,
                                Some(card.id.clone()),
                                "failed",
                                reason.clone(),
                            )?;
                            return Err(reason);
                        }

                        mutate_board(&board, workspace, |board| {
                            board.record_merge_conflict(
                                &card.id,
                                "Codex approved the isolated implementation, but the merge back to the main workspace conflicted.".to_string(),
                                findings.clone(),
                                conflict.clone(),
                            )?;
                            board.finish_active_subtask(&card.id);
                            Ok(())
                        })
                        .await?;
                        emit_blackboard(
                            workspace,
                            app_handle,
                            window_label,
                            Some(card.id.clone()),
                            "needs_fix",
                            format!(
                                "Subtask {} passed review but hit merge conflicts. Claude will retry from a fresh isolated workspace.",
                                card.id
                            ),
                        )?;
                        return Ok(AttemptResolution::Retry);
                    }
                }
            }

            mutate_board(&board, workspace, |board| {
                board.record_review(&card.id, false, review.summary.clone(), review.findings.clone())?;
                board.finish_active_subtask(&card.id);
                Ok(())
            })
            .await?;

            if attempt >= MAX_SUBTASK_ATTEMPTS {
                let reason = format!(
                    "Subtask {} failed inline review after {} attempts: {}",
                    card.id, MAX_SUBTASK_ATTEMPTS, review.summary
                );
                mutate_board(&board, workspace, |board| board.mark_failed(&card.id, reason.clone()))
                    .await?;
                emit_blackboard(
                    workspace,
                    app_handle,
                    window_label,
                    Some(card.id.clone()),
                    "failed",
                    reason.clone(),
                )?;
                return Err(reason);
            }

            emit_blackboard(
                workspace,
                app_handle,
                window_label,
                Some(card.id.clone()),
                "needs_fix",
                format!(
                    "Codex rejected {} on attempt {}. Claude will retry in a fresh isolated workspace using the shared blackboard findings.",
                    card.id, attempt
                ),
            )?;
            Ok(AttemptResolution::Retry)
        }
        .await;

        let clear_board_err = mutate_board(&board, workspace, |board| {
            board.set_isolated_workspace(&subtask_id, None)
        })
        .await
        .err();
        let finish_active_err = if attempt_result.is_err() {
            mutate_board(&board, workspace, |board| {
                board.finish_active_subtask(&subtask_id);
                Ok(())
            })
            .await
            .err()
        } else {
            None
        };
        let cleanup_err = cleanup_isolated_workspace(&isolated.root)
            .and_then(|()| cleanup_isolated_workspace(&isolated.base_dir))
            .err();

        if let Some(err) = clear_board_err.or(finish_active_err).or(cleanup_err) {
            return match attempt_result {
                Ok(_) => Err(err),
                Err(primary) => Err(format!("{primary} (cleanup error: {err})")),
            };
        }

        match attempt_result? {
            AttemptResolution::Completed => return Ok(()),
            AttemptResolution::Retry => continue,
        }
    }
}

async fn mutate_board<T, F>(
    board: &std::sync::Arc<Mutex<Blackboard>>,
    workspace: &str,
    mutator: F,
) -> Result<T, String>
where
    F: FnOnce(&mut Blackboard) -> Result<T, String>,
{
    let mut board = board.lock().await;
    let value = mutator(&mut board)?;
    board.persist(workspace)?;
    Ok(value)
}

async fn read_card(
    board: &std::sync::Arc<Mutex<Blackboard>>,
    subtask_id: &str,
) -> Result<SubtaskCard, String> {
    let board = board.lock().await;
    board.subtask(subtask_id).cloned()
}

#[derive(Clone, Debug)]
struct ActiveSubtaskMeta {
    can_run_in_parallel: bool,
    parallel_group: Option<String>,
}

pub(super) enum AttemptResolution {
    Completed,
    Retry,
}

async fn ordinal_for_subtask(
    board: &std::sync::Arc<Mutex<Blackboard>>,
    subtask_id: &str,
) -> Result<usize, String> {
    let board = board.lock().await;
    board
        .subtasks
        .iter()
        .position(|card| card.id == subtask_id)
        .map(|index| index + 1)
        .ok_or_else(|| format!("Unknown subtask order for {subtask_id}"))
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::blackboard::{SubtaskKind, SubtaskState};

    fn test_card(id: &str, can_run_in_parallel: bool, parallel_group: Option<&str>) -> SubtaskCard {
        SubtaskCard {
            id: id.to_string(),
            title: format!("Title {id}"),
            description: "desc".to_string(),
            kind: SubtaskKind::Feature,
            depends_on: Vec::new(),
            can_run_in_parallel,
            parallel_group: parallel_group.map(ToOwned::to_owned),
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
        }
    }

    #[test]
    fn can_spawn_subtask_allows_parallel_work_in_different_groups() {
        let card = test_card("F2", true, Some("ui"));
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                can_run_in_parallel: true,
                parallel_group: Some("api".to_string()),
            },
        )]);

        assert!(can_spawn_subtask(&card, &active));
    }

    #[test]
    fn can_spawn_subtask_blocks_same_parallel_group() {
        let card = test_card("F2", true, Some("api"));
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                can_run_in_parallel: true,
                parallel_group: Some("api".to_string()),
            },
        )]);

        assert!(!can_spawn_subtask(&card, &active));
    }

    #[test]
    fn can_spawn_subtask_blocks_when_active_task_requires_exclusive_lane() {
        let card = test_card("F2", true, Some("ui"));
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                can_run_in_parallel: false,
                parallel_group: None,
            },
        )]);

        assert!(!can_spawn_subtask(&card, &active));
    }

    #[test]
    fn can_spawn_subtask_blocks_non_parallel_candidate_when_lane_is_busy() {
        let card = test_card("F2", false, None);
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                can_run_in_parallel: true,
                parallel_group: Some("api".to_string()),
            },
        )]);

        assert!(!can_spawn_subtask(&card, &active));
    }

}
