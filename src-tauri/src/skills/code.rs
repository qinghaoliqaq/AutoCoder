/// Code skill — per-subtask implementation loop driven by a shared blackboard.
///
/// Orchestrates: scheduling → isolated workspace → Claude implement → verifier
/// → Codex review → three-way merge back to main workspace.
///
/// The main `run_subtask` function delegates to focused phase functions:
///   `run_implementation_phase`  — vendored skill selection + Claude implementation
///   `run_verification_phase`    — verifier check against acceptance criteria
///   `run_review_and_merge_phase` — Codex review + three-way merge back
///
/// Heavy-lifting is delegated to submodules:
///   isolated_workspace — fork / sync / cleanup / snapshot / diff
///   merge_engine       — three-way line-level merge
///   code_prompts       — prompt builders and output parsers
///   code_events        — Tauri event emission and evidence recording
use super::{
    blackboard::{tick_plan_checkbox, Blackboard, SubtaskCard, BLACKBOARD_MD},
    build_gate,
    code_events::{emit_blackboard, emit_vendored_skill_log},
    code_prompts::{
        build_fix_prompt, build_implement_prompt, build_review_prompt, parse_implementation_report,
        parse_review_report,
    },
    isolated_workspace::{
        cleanup_isolated_workspace, create_isolated_workspace, relative_paths_from_root,
        snapshot_workspace, sync_coordination_files, workspace_changes, IsolatedWorkspace,
    },
    merge_engine::merge_isolated_workspace,
    vendored::{load as load_vendored_skill, select_for_subtask},
};
use super::{evidence, planning_schema::SubtaskAcceptance, verifier};
use super::planning_schema::read_plan_acceptance_lenient;
use crate::{config::AppConfig, prompts::Prompts, tool_runner};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinSet};
use tokio_util::sync::CancellationToken;

const MAX_SUBTASK_ATTEMPTS: u32 = 5;

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
    let acceptance_by_subtask = Arc::new(
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

    let _ = emit_blackboard(
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
    );
    if let Some(warning) = acceptance_warning {
        let _ = emit_blackboard(
            workspace,
            app_handle,
            window_label,
            None,
            "acceptance_unavailable",
            format!(
                "Structured acceptance data is unavailable, so code mode is falling back to PLAN.md and blackboard-only review: {warning}"
            ),
        );
    }

    let base_prompt = Prompts::render(&prompts.code_claude, &[("task", task)]);
    let shared_board = Arc::new(Mutex::new(board));
    let merge_lock = Arc::new(Mutex::new(()));

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

    // Track subtask failures without cancelling other workers.  Only a
    // JoinSet panic (worker crash) is treated as truly fatal.

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
            // Some subtasks are not done — check why.
            let failed: Vec<_> = board
                .subtasks
                .iter()
                .filter(|card| matches!(card.status, super::blackboard::SubtaskState::Failed))
                .map(|card| card.id.clone())
                .collect();
            if !failed.is_empty() {
                let done_count = board
                    .subtasks
                    .iter()
                    .filter(|card| matches!(card.status, super::blackboard::SubtaskState::Done))
                    .count();
                break Some(format!(
                    "Subtask(s) {} failed after exhausting retries. {}/{} subtasks completed successfully. \
                     Tell me to continue and I will retry the failed ones.",
                    failed.join(", "),
                    done_count,
                    total,
                ));
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
            // Single subtask failure — log it but let other subtasks continue.
            // run_subtask marks the subtask as Failed on the board before
            // returning Err, so dependents won't be blocked indefinitely.
            tracing::warn!(
                subtask = %subtask_id,
                "Subtask exhausted retries: {err}"
            );
            let _ = emit_blackboard(
                workspace,
                app_handle,
                window_label,
                Some(subtask_id.clone()),
                "subtask_failed",
                format!("Subtask {} failed: {err}. Other subtasks continue.", subtask_id),
            );
        }
    };

    // Drain any remaining workers (should only happen on JoinSet panic).
    while let Some(joined) = join_set.join_next().await {
        if fatal_error.is_none() {
            if let Ok((id, Err(err))) = joined {
                tracing::warn!(subtask = %id, "Late subtask failure: {err}");
            }
        }
    }

    if let Some(err) = fatal_error {
        return Err(err);
    }

    let mut board = shared_board.lock().await;
    board.complete_if_finished();
    board.persist(workspace)?;
    let _ = emit_blackboard(
        workspace,
        app_handle,
        window_label,
        None,
        "completed",
        "All planned subtasks passed inline review and were merged from isolated workspaces."
            .to_string(),
    );
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
    board: &Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: &Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: &Arc<Mutex<()>>,
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
    // A non-parallel subtask must run alone — block if anything else is active.
    if !card.can_run_in_parallel {
        return false;
    }
    // If any active subtask is non-parallel, only block new non-parallel tasks;
    // parallel tasks can still proceed alongside it.
    // (Previously this blocked ALL new spawns, starving the pipeline.)
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
    board: Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: Arc<Mutex<()>>,
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

/// Context passed through the subtask attempt phases to avoid deep parameter lists.
struct AttemptContext<'a> {
    task: &'a str,
    workspace: &'a str,
    context: Option<&'a str>,
    config: &'a AppConfig,
    base_prompt: &'a str,
    board: Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: Arc<Mutex<()>>,
    window_label: &'a str,
    app_handle: &'a tauri::AppHandle,
    token: CancellationToken,
    ordinal: usize,
    total: usize,
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
    board: Arc<Mutex<Blackboard>>,
    acceptance_by_subtask: Arc<HashMap<String, SubtaskAcceptance>>,
    merge_lock: Arc<Mutex<()>>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let subtask_id = initial_card.id.clone();
    let ctx = AttemptContext {
        task,
        workspace,
        context,
        config,
        base_prompt,
        board,
        acceptance_by_subtask,
        merge_lock,
        window_label,
        app_handle,
        token,
        ordinal,
        total,
    };

    // When a subtask fails review (Retry), we keep its isolated workspace so
    // Claude can fix the existing code in-place instead of reimplementing from
    // scratch.  Only transient errors (Err) get a fresh workspace.
    let mut reuse_isolated: Option<IsolatedWorkspace> = None;

    loop {
        let attempt = match mutate_board(&ctx.board, workspace, |board| {
            board.begin_attempt(&subtask_id)
        })
        .await
        {
            Ok(a) => a,
            Err(e) => {
                // If begin_attempt modified in-memory state but persist failed,
                // the subtask is InProgress in memory.  Mark it failed so the
                // scheduler detects it instead of a silent deadlock.
                let _ = mutate_board(&ctx.board, workspace, |board| {
                    board.mark_failed(
                        &subtask_id,
                        format!("begin_attempt failed: {e}"),
                    )
                })
                .await;
                return Err(e);
            }
        };

        // Setup phase: create/reuse isolated workspace.  If any step fails,
        // we must clean up the active-subtask tracking before propagating.
        let isolated = match setup_isolated_workspace(
            &ctx,
            &subtask_id,
            attempt,
            reuse_isolated.take(),
        )
        .await
        {
            Ok(iso) => iso,
            Err(e) => {
                // Setup failed — remove from active set so the scheduler
                // doesn't think we're still running.
                let _ = mutate_board(&ctx.board, workspace, |board| {
                    board.finish_active_subtask(&subtask_id);
                    Ok(())
                })
                .await;
                if attempt < MAX_SUBTASK_ATTEMPTS {
                    tracing::warn!(
                        subtask = %subtask_id,
                        attempt,
                        "Workspace setup failed, will retry fresh: {e}"
                    );
                    // Clear stale review findings — same rationale as the
                    // transient error path: fresh workspace + fix prompt = confusion.
                    let _ = mutate_board(&ctx.board, workspace, |board| {
                        let card = board.subtask_mut(&subtask_id)?;
                        card.review_findings.clear();
                        card.merge_conflict = None;
                        Ok(())
                    })
                    .await;
                    continue;
                }
                let _ = mutate_board(&ctx.board, workspace, |board| {
                    board.mark_failed(&subtask_id, format!("Workspace setup failed after {MAX_SUBTASK_ATTEMPTS} attempts: {e}"))
                })
                .await;
                return Err(e);
            }
        };

        let attempt_result = run_single_attempt(&ctx, &subtask_id, attempt, &isolated).await;

        let clear_board_err = mutate_board(&ctx.board, workspace, |board| {
            board.set_isolated_workspace(&subtask_id, None)
        })
        .await
        .err();
        let finish_active_err = if attempt_result.is_err() {
            mutate_board(&ctx.board, workspace, |board| {
                board.finish_active_subtask(&subtask_id);
                Ok(())
            })
            .await
            .err()
        } else {
            None
        };

        // Only clean up the workspace when we won't reuse it.
        let should_reuse = matches!(attempt_result, Ok(AttemptResolution::Retry));
        let cleanup_err = if should_reuse {
            None
        } else {
            cleanup_isolated_workspace(&isolated.root)
                .and_then(|()| cleanup_isolated_workspace(&isolated.base_dir))
                .err()
        };

        if let Some(err) = clear_board_err.or(finish_active_err).or(cleanup_err) {
            match &attempt_result {
                Ok(_) => {
                    // Cleanup error is non-critical when the attempt itself
                    // succeeded (Completed) or requested retry (Retry).
                    // Returning Err here would prevent the retry loop and
                    // cause a launched_ids deadlock since the subtask is
                    // NeedsFix but can never be re-spawned in this session.
                    tracing::warn!(
                        subtask = %subtask_id,
                        "Non-fatal cleanup error (attempt result OK): {err}"
                    );
                }
                Err(primary) => {
                    // Mark failed so the scheduler doesn't leave this subtask
                    // stuck in InProgress (which would block dependents).
                    let _ = mutate_board(&ctx.board, workspace, |board| {
                        board.mark_failed(&subtask_id, format!("{primary} (cleanup error: {err})"))
                    })
                    .await;
                    return Err(format!("{primary} (cleanup error: {err})"));
                }
            }
        }

        match attempt_result {
            Ok(AttemptResolution::Completed) => return Ok(()),
            Ok(AttemptResolution::Retry) => {
                // Keep the workspace so Claude fixes existing code next attempt.
                reuse_isolated = Some(isolated);
                continue;
            }
            Err(e) if attempt < MAX_SUBTASK_ATTEMPTS => {
                // Transient error (Claude/Codex crash, network issue, etc.) —
                // retry with a fresh workspace.
                tracing::warn!(
                    subtask = %subtask_id,
                    attempt,
                    "Subtask attempt errored, will retry fresh: {e}"
                );
                // Clear stale review findings from the previous Retry iteration.
                // A fresh workspace has no prior code, so using the fix prompt
                // ("do NOT rewrite from scratch") would mislead Claude into
                // making targeted edits on a blank slate → guaranteed failure.
                let _ = mutate_board(&ctx.board, workspace, |board| {
                    let card = board.subtask_mut(&subtask_id)?;
                    card.review_findings.clear();
                    card.merge_conflict = None;
                    Ok(())
                })
                .await;
                let _ = emit_blackboard(
                    workspace,
                    app_handle,
                    window_label,
                    Some(subtask_id.clone()),
                    "needs_fix",
                    format!(
                        "Subtask {} attempt {} hit a transient error: {e}. Retrying fresh.",
                        subtask_id, attempt
                    ),
                );
                continue;
            }
            Err(e) => {
                // All retries exhausted with transient errors — mark as
                // Failed so the scheduler can detect it (instead of leaving
                // the subtask stuck in InProgress which blocks dependents).
                let _ = mutate_board(&ctx.board, workspace, |board| {
                    board.mark_failed(&subtask_id, format!("Exhausted {MAX_SUBTASK_ATTEMPTS} attempts: {e}"))
                })
                .await;
                return Err(e);
            }
        }
    }
}

/// Set up the isolated workspace for an attempt — create fresh or reuse previous.
async fn setup_isolated_workspace(
    ctx: &AttemptContext<'_>,
    subtask_id: &str,
    attempt: u32,
    reuse: Option<IsolatedWorkspace>,
) -> Result<IsolatedWorkspace, String> {
    let isolated = if let Some(prev) = reuse {
        tracing::info!(
            subtask = %subtask_id,
            attempt,
            "Reusing previous isolated workspace for fix attempt"
        );
        sync_coordination_files(ctx.workspace, &prev.root)?;
        prev
    } else {
        create_isolated_workspace(ctx.workspace, subtask_id, attempt)?
    };
    mutate_board(&ctx.board, ctx.workspace, |board| {
        board.set_isolated_workspace(subtask_id, Some(isolated.root.display().to_string()))
    })
    .await?;
    Ok(isolated)
}

/// Execute a single attempt of a subtask: implement → build gate → verify → review → merge.
async fn run_single_attempt(
    ctx: &AttemptContext<'_>,
    subtask_id: &str,
    attempt: u32,
    isolated: &IsolatedWorkspace,
) -> Result<AttemptResolution, String> {
    let card = read_card(&ctx.board, subtask_id).await?;

    // Phase 0: Emit start event.
    let summary = if attempt == 1 {
        format!(
            "Subtask {}/{}: {} is now implementing {} in isolated workspace {}.",
            ctx.ordinal,
            ctx.total,
            card.id,
            card.title,
            isolated.root.display()
        )
    } else {
        format!(
            "Subtask {}/{}: {} needs another pass. Claude is fixing {} in isolated workspace {} using Codex findings from {}.",
            ctx.ordinal, ctx.total, card.id, card.title, isolated.root.display(), BLACKBOARD_MD
        )
    };
    let _ = emit_blackboard(
        ctx.workspace,
        ctx.app_handle,
        ctx.window_label,
        Some(card.id.clone()),
        "subtask_started",
        summary,
    );

    // Phase 1: Claude implementation.
    let (implementation, acceptance) =
        run_implementation_phase(ctx, &card, attempt, isolated).await?;

    // Phase 1.5: Build gate — compile/type-check before review.
    if let Some(resolution) = run_build_gate_phase(ctx, &card, attempt, isolated).await? {
        return Ok(resolution);
    }

    // Phase 2: Verification.
    let verifier_warnings =
        run_verification_phase(ctx, &card, attempt, isolated, &acceptance, &implementation).await?;
    let Some(verifier_warnings) = verifier_warnings else {
        return Ok(AttemptResolution::Retry);
    };

    // Phase 3: Codex review + merge.
    run_review_and_merge_phase(
        ctx,
        &card,
        attempt,
        isolated,
        &acceptance,
        &verifier_warnings,
    )
    .await
}

/// Phase 1 — Select vendored skill (if any), build prompt, run Claude, record implementation.
async fn run_implementation_phase(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
    attempt: u32,
    isolated: &IsolatedWorkspace,
) -> Result<
    (
        super::code_prompts::ImplementationReport,
        Option<SubtaskAcceptance>,
    ),
    String,
> {
    let vendored_skill = resolve_vendored_skill(ctx, card)?;

    sync_coordination_files(ctx.workspace, &isolated.root)?;
    let acceptance = ctx.acceptance_by_subtask.get(&card.id).cloned();

    let claude_prompt = if card.review_findings.is_empty() {
        build_implement_prompt(
            ctx.base_prompt,
            ctx.task,
            card,
            acceptance.as_ref(),
            vendored_skill.as_ref(),
        )
    } else {
        build_fix_prompt(
            ctx.base_prompt,
            ctx.task,
            card,
            acceptance.as_ref(),
            vendored_skill.as_ref(),
        )
    };
    let claude_prompt = super::inject_context(ctx.context, claude_prompt);
    // Inject evidence history for retries so Claude knows what failed before.
    let claude_prompt = if attempt > 1 {
        match evidence::build_subtask_context(ctx.workspace, &card.id) {
            Some(evidence_ctx) => format!("{claude_prompt}\n\n---\n\n{evidence_ctx}"),
            None => claude_prompt,
        }
    } else {
        claude_prompt
    };

    let claude_output = tool_runner::run_subtask(
        ctx.config,
        "You are a senior developer implementing a subtask. \
         Use the editor and bash tools to write code. \
         Follow the plan and acceptance criteria precisely.",
        &claude_prompt,
        Some(isolated.root.to_string_lossy().as_ref()),
        ctx.window_label,
        ctx.app_handle,
        ctx.token.clone(),
        &card.id,
    )
    .await?;

    let isolated_after = snapshot_workspace(&isolated.root);
    let isolated_changes = workspace_changes(&isolated.base_snapshot, &isolated_after);
    let observed_files =
        relative_paths_from_root(&isolated.root, &isolated_changes.changed_or_created);
    let implementation = parse_implementation_report(&claude_output, &observed_files, &card.id);

    mutate_board(&ctx.board, ctx.workspace, |board| {
        board.record_implementation(
            &card.id,
            implementation.summary.clone(),
            implementation.files_touched.clone(),
        )
    })
    .await?;

    let _ = emit_blackboard(
        ctx.workspace,
        ctx.app_handle,
        ctx.window_label,
        Some(card.id.clone()),
        "implemented",
        format!(
            "Claude finished {} attempt {} in isolation. Verifier is now checking the subtask.",
            card.id, attempt
        ),
    );

    Ok((implementation, acceptance))
}

/// Resolve the vendored skill for a subtask, emitting appropriate events.
fn resolve_vendored_skill(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
) -> Result<Option<super::vendored::VendoredSkill>, String> {
    match (
        ctx.config.features.vendored_skills,
        select_for_subtask(card),
    ) {
        (true, Some(skill_id)) => match load_vendored_skill(skill_id) {
            Ok(skill) => {
                let _ = emit_vendored_skill_log(ctx.app_handle, ctx.window_label, "claude", &skill, card);
                let _ = emit_blackboard(
                    ctx.workspace,
                    ctx.app_handle,
                    ctx.window_label,
                    Some(card.id.clone()),
                    "vendored_skill_selected",
                    format!(
                        "Subtask {} is using packaged helper skill {}.",
                        card.id,
                        skill.id.label()
                    ),
                );
                Ok(Some(skill))
            }
            Err(err) => {
                let _ = emit_blackboard(
                    ctx.workspace,
                    ctx.app_handle,
                    ctx.window_label,
                    Some(card.id.clone()),
                    "vendored_skill_unavailable",
                    format!(
                        "Packaged helper skill for {} is unavailable, continuing without it: {}",
                        card.id, err
                    ),
                );
                Ok(None)
            }
        },
        (false, Some(_)) => {
            let _ = emit_blackboard(
                ctx.workspace, ctx.app_handle, ctx.window_label,
                Some(card.id.clone()), "vendored_skill_disabled",
                format!("Packaged helper skills are disabled in config. Subtask {} is continuing without them.", card.id),
            );
            Ok(None)
        }
        (_, None) => Ok(None),
    }
}

/// Phase 1.5 — Run compile/type-check commands in the isolated workspace.
/// Returns `Some(Retry)` if the build failed and the subtask should retry,
/// or `None` if the gate passed (or was skipped) and phases can continue.
async fn run_build_gate_phase(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
    attempt: u32,
    isolated: &IsolatedWorkspace,
) -> Result<Option<AttemptResolution>, String> {
    if !ctx.config.features.build_gate {
        return Ok(None);
    }

    let commands = build_gate::detect_build_commands(&isolated.root);
    if commands.is_empty() {
        return Ok(None);
    }

    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    let _ = emit_blackboard(
        ctx.workspace,
        ctx.app_handle,
        ctx.window_label,
        Some(card.id.clone()),
        "build_gate_running",
        format!(
            "Build gate: running {} for subtask {}.",
            labels.join(", "),
            card.id
        ),
    );

    let result = build_gate::run_build_gate(&isolated.root, &commands).await;

    if result.passed {
        let _ = emit_blackboard(
            ctx.workspace,
            ctx.app_handle,
            ctx.window_label,
            Some(card.id.clone()),
            "build_gate_passed",
            format!(
                "Build gate passed for {} attempt {}. Proceeding to verification.",
                card.id, attempt
            ),
        );
        return Ok(None);
    }

    // Build failed — record the compile errors as review findings so the
    // fix prompt can include them, then decide whether to retry or abort.
    let failure_summary = result.failure_summary();
    let finding = format!("Build gate failed:\n{failure_summary}");

    mutate_board(&ctx.board, ctx.workspace, |board| {
        board.record_review(
            &card.id,
            false,
            "Build gate: compile/type-check failed.".to_string(),
            vec![finding],
        )?;
        board.finish_active_subtask(&card.id);
        Ok(())
    })
    .await?;

    let _ = emit_blackboard(
        ctx.workspace,
        ctx.app_handle,
        ctx.window_label,
        Some(card.id.clone()),
        "build_gate_failed",
        format!(
            "Build gate failed for {} attempt {}: compile errors detected. {}",
            card.id,
            attempt,
            if attempt >= MAX_SUBTASK_ATTEMPTS {
                "Max attempts reached."
            } else {
                "Claude will retry with error context."
            }
        ),
    );

    if attempt >= MAX_SUBTASK_ATTEMPTS {
        let reason = format!(
            "Subtask {} failed build gate after {} attempts.",
            card.id, MAX_SUBTASK_ATTEMPTS
        );
        mutate_board(&ctx.board, ctx.workspace, |board| {
            board.mark_failed(&card.id, reason.clone())
        })
        .await?;
        return Err(reason);
    }

    Ok(Some(AttemptResolution::Retry))
}

/// Phase 2 — Run verifier; on failure, record and possibly abort.
/// Returns `Some(warnings)` if passed, `None` if failed (caller should retry).
async fn run_verification_phase(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
    attempt: u32,
    isolated: &IsolatedWorkspace,
    acceptance: &Option<SubtaskAcceptance>,
    implementation: &super::code_prompts::ImplementationReport,
) -> Result<Option<Vec<String>>, String> {
    let verifier_result = verifier::run_and_persist(
        ctx.workspace,
        &isolated.root,
        card,
        acceptance.as_ref(),
        &implementation.files_touched,
        &implementation.summary,
    )?;

    if verifier_result.passed {
        let _ = emit_blackboard(
            ctx.workspace,
            ctx.app_handle,
            ctx.window_label,
            Some(card.id.clone()),
            "verifier_passed",
            format!(
                "Verifier passed {} attempt {}. Codex is now reviewing the subtask.",
                card.id, attempt
            ),
        );
        return Ok(Some(verifier_result.warnings));
    }

    // Verifier failed — record findings and decide whether to retry or abort.
    mutate_board(&ctx.board, ctx.workspace, |board| {
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
    let _ = emit_blackboard(
        ctx.workspace,
        ctx.app_handle,
        ctx.window_label,
        Some(card.id.clone()),
        "verifier_failed",
        format!(
            "Verifier blocked {} attempt {} before Codex review: {}",
            card.id, attempt, verifier_result.summary
        ),
    );

    if attempt >= MAX_SUBTASK_ATTEMPTS {
        let reason = format!(
            "Subtask {} failed verifier after {} attempts: {}",
            card.id, MAX_SUBTASK_ATTEMPTS, verifier_result.summary
        );
        mutate_board(&ctx.board, ctx.workspace, |board| {
            board.mark_failed(&card.id, reason.clone())
        })
        .await?;
        let _ = emit_blackboard(
            ctx.workspace,
            ctx.app_handle,
            ctx.window_label,
            Some(card.id.clone()),
            "failed",
            reason.clone(),
        );
        return Err(reason);
    }

    let _ = emit_blackboard(
        ctx.workspace, ctx.app_handle, ctx.window_label,
        Some(card.id.clone()), "needs_fix",
        format!("Verifier rejected {} on attempt {}. Claude will fix the code in-place using the verifier findings.", card.id, attempt),
    );
    Ok(None)
}

/// Phase 3 — Run Codex review, then merge on success.
async fn run_review_and_merge_phase(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
    attempt: u32,
    isolated: &IsolatedWorkspace,
    acceptance: &Option<SubtaskAcceptance>,
    verifier_warnings: &[String],
) -> Result<AttemptResolution, String> {
    let review_card = read_card(&ctx.board, &card.id).await?;
    sync_coordination_files(ctx.workspace, &isolated.root)?;
    let review_prompt = super::inject_context(
        ctx.context,
        build_review_prompt(
            ctx.task,
            &review_card,
            acceptance.as_ref(),
            verifier_warnings,
        ),
    );
    let review_output = tool_runner::run_read_only_subtask(
        ctx.config,
        "You are a code reviewer checking a subtask implementation. \
         Read and verify the code against acceptance criteria. \
         This is a read-only review — only view, grep, and glob tools are available.",
        &review_prompt,
        Some(isolated.root.to_string_lossy().as_ref()),
        ctx.window_label,
        ctx.app_handle,
        ctx.token.clone(),
        &card.id,
    )
    .await?;
    let review = parse_review_report(&review_output);

    if review.passed {
        return apply_merge(ctx, card, &review_card, attempt, isolated, &review).await;
    }

    // Review rejected — record and decide whether to retry or abort.
    mutate_board(&ctx.board, ctx.workspace, |board| {
        board.record_review(
            &card.id,
            false,
            review.summary.clone(),
            review.findings.clone(),
        )?;
        board.finish_active_subtask(&card.id);
        Ok(())
    })
    .await?;

    if attempt >= MAX_SUBTASK_ATTEMPTS {
        let reason = format!(
            "Subtask {} failed inline review after {} attempts: {}",
            card.id, MAX_SUBTASK_ATTEMPTS, review.summary
        );
        mutate_board(&ctx.board, ctx.workspace, |board| {
            board.mark_failed(&card.id, reason.clone())
        })
        .await?;
        let _ = emit_blackboard(
            ctx.workspace,
            ctx.app_handle,
            ctx.window_label,
            Some(card.id.clone()),
            "failed",
            reason.clone(),
        );
        return Err(reason);
    }

    let _ = emit_blackboard(
        ctx.workspace, ctx.app_handle, ctx.window_label,
        Some(card.id.clone()), "needs_fix",
        format!("Codex rejected {} on attempt {}. Claude will fix the existing code in-place using the shared blackboard findings.", card.id, attempt),
    );
    Ok(AttemptResolution::Retry)
}

/// Merge the isolated workspace back to main, handling conflicts.
async fn apply_merge(
    ctx: &AttemptContext<'_>,
    card: &SubtaskCard,
    review_card: &SubtaskCard,
    attempt: u32,
    isolated: &IsolatedWorkspace,
    review: &super::code_prompts::ReviewReport,
) -> Result<AttemptResolution, String> {
    // Serialize merges so parallel subtasks don't race on the main workspace.
    let _merge_guard = ctx.merge_lock.lock().await;
    match merge_isolated_workspace(ctx.workspace, isolated) {
        Ok(merged_files) => {
            mutate_board(&ctx.board, ctx.workspace, |board| {
                board.record_implementation(
                    &card.id,
                    review_card
                        .latest_implementation
                        .clone()
                        .unwrap_or_else(|| {
                            "Implementation merged from isolated workspace.".to_string()
                        }),
                    merged_files.clone(),
                )?;
                board.record_review(&card.id, true, review.summary.clone(), Vec::new())?;
                board.finish_active_subtask(&card.id);
                board.complete_if_finished();
                Ok(())
            })
            .await?;
            if let Err(e) = tick_plan_checkbox(ctx.workspace, &card.id) {
                tracing::warn!(subtask = %card.id, "Failed to tick plan checkbox (non-fatal): {e}");
            }
            let _ = emit_blackboard(
                ctx.workspace,
                ctx.app_handle,
                ctx.window_label,
                Some(card.id.clone()),
                "passed",
                format!(
                    "Subtask {} passed Codex review and merged cleanly from isolated workspace.",
                    card.id
                ),
            );
            Ok(AttemptResolution::Completed)
        }
        Err(conflict) => {
            let mut findings = review.findings.clone();
            findings.push(conflict.clone());

            if attempt >= MAX_SUBTASK_ATTEMPTS {
                let reason = format!(
                    "Subtask {} hit merge conflicts after {} attempts: {}",
                    card.id, MAX_SUBTASK_ATTEMPTS, conflict
                );
                mutate_board(&ctx.board, ctx.workspace, |board| {
                    board.mark_failed(&card.id, reason.clone())
                })
                .await?;
                let _ = emit_blackboard(
                    ctx.workspace,
                    ctx.app_handle,
                    ctx.window_label,
                    Some(card.id.clone()),
                    "failed",
                    reason.clone(),
                );
                return Err(reason);
            }

            mutate_board(&ctx.board, ctx.workspace, |board| {
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
            let _ = emit_blackboard(
                ctx.workspace, ctx.app_handle, ctx.window_label,
                Some(card.id.clone()), "needs_fix",
                format!("Subtask {} passed review but hit merge conflicts. Claude will fix the code in-place to resolve conflicts.", card.id),
            );
            Ok(AttemptResolution::Retry)
        }
    }
}

async fn mutate_board<T, F>(
    board: &Arc<Mutex<Blackboard>>,
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
    board: &Arc<Mutex<Blackboard>>,
    subtask_id: &str,
) -> Result<SubtaskCard, String> {
    let board = board.lock().await;
    board.subtask(subtask_id).cloned()
}

#[derive(Clone, Debug)]
struct ActiveSubtaskMeta {
    parallel_group: Option<String>,
}

pub(super) enum AttemptResolution {
    Completed,
    Retry,
}

async fn ordinal_for_subtask(
    board: &Arc<Mutex<Blackboard>>,
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
            attempted_fixes: Vec::new(),
        }
    }

    #[test]
    fn can_spawn_subtask_allows_parallel_work_in_different_groups() {
        let card = test_card("F2", true, Some("ui"));
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
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
                parallel_group: Some("api".to_string()),
            },
        )]);

        assert!(!can_spawn_subtask(&card, &active));
    }

    #[test]
    fn can_spawn_subtask_allows_parallel_task_alongside_exclusive() {
        // Parallel tasks can proceed alongside a non-parallel task because each
        // subtask runs in an isolated workspace; merges are serialized separately.
        let card = test_card("F2", true, Some("ui"));
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                parallel_group: None,
            },
        )]);

        assert!(can_spawn_subtask(&card, &active));
    }

    #[test]
    fn can_spawn_subtask_blocks_non_parallel_candidate_when_lane_is_busy() {
        let card = test_card("F2", false, None);
        let active = HashMap::from([(
            "F1".to_string(),
            ActiveSubtaskMeta {
                parallel_group: Some("api".to_string()),
            },
        )]);

        assert!(!can_spawn_subtask(&card, &active));
    }
}
