/// Review skill — static analysis pipeline.
///
/// Phases (run sequentially by the frontend):
///   plan_check       — Claude + Codex verify all features are implemented
///   security         — Claude deep audit (OWASP/STRIDE/LLM) + Codex cross-check
///   specialist_review — 4 parallel specialists (security/performance/API/testing)
///   design_review    — visual consistency, spacing, color, typography, a11y
///   cleanup          — Claude removes dead code and unused imports
///
/// Dynamic integration testing (env setup, server start, curl suite, fixes, document)
/// has been moved to the test skill (test_skill.rs / run_phase).
///
/// Each phase emits "review-phase-result" when it finishes.
use super::{runners, ReviewPhaseResult};
use crate::evidence::{self, EvidenceEvent};
use crate::prompts::Prompts;
use chrono::Utc;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run_phase(
    phase: &str,
    task: &str,
    issue: Option<&str>,
    workspace: Option<&str>,
    context: Option<&str>,
    prompts: Option<&Prompts>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let _ = issue; // unused in static phases

    let (passed, found_issue) = match phase {
        // ── Plan completion check ─────────────────────────────────────────────
        // Claude and Codex check in parallel; missing lists are unioned.
        // Claude then ticks completed items in PLAN.md.
        "plan_check" => {
            let claude_prompt = format!(
                "Verify that every item in the project plan has been implemented.\n\
                 Task context: {task}\n\n\
                 Steps:\n\
                 1. Check if PLAN.md exists in the current directory.\n\
                    If it does NOT exist: print 'No PLAN.md — skipping.' and output MISSING:[]\n\n\
                 2. Read PLAN.md. Extract every checklist item — lines that match:\n\
                    - `- [ ] **F<n>. ...`  (backend feature)\n\
                    - `- [ ] **P<n>. ...`  (UI screen / view)\n\
                    Fall back to any numbered list or bullet item if none found.\n\n\
                 3. For each item, search the source code to confirm it is implemented.\n\
                    Mark each as ✅ DONE or ❌ MISSING.\n\n\
                 4. Print a concise table:\n\
                    | ID | Name | Status |\n\
                    |----|------|--------|\n\
                    | F1 | User Registration | ✅ DONE |\n\
                    | P2 | Dashboard screen  | ❌ MISSING |\n\n\
                 5. For every item marked ✅ DONE: edit PLAN.md and change `- [ ]` to `- [x]`\n\
                    on that specific line only. Do NOT modify any other content.\n\n\
                 At the very end output exactly one line:\n\
                 MISSING:[F2 User Login, P3 Dashboard]   — comma-separated IDs+names of MISSING items\n\
                 MISSING:[]                               — if everything is implemented"
            );
            let codex_prompt = format!(
                "Verify that every item in the project plan has been implemented.\n\
                 Task context: {task}\n\n\
                 Steps:\n\
                 1. Check if PLAN.md exists. If not: output MISSING:[] and stop.\n\n\
                 2. Read PLAN.md. Extract every checklist item:\n\
                    - `- [ ] **F<n>. ...` or `- [ ] **P<n>. ...`\n\
                    Fall back to any numbered/bullet items if none found.\n\n\
                 3. For each item, search the source code to verify it is implemented.\n\
                    Mark each as ✅ DONE or ❌ MISSING.\n\n\
                 4. Print a concise table:\n\
                    | ID | Name | Status |\n\
                    |----|------|--------|\n\n\
                 At the very end output exactly one line:\n\
                 MISSING:[F2 User Login, P3 Dashboard]   — comma-separated IDs+names of MISSING items\n\
                 MISSING:[]                               — if everything is implemented"
            );

            // Inject plan context into both prompts
            let claude_prompt = super::inject_context(context, claude_prompt);
            let codex_prompt = super::inject_context(context, codex_prompt);

            // Run both agents in parallel
            let (claude_result, codex_result) = tokio::join!(
                runners::claude(
                    &claude_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone()
                ),
                runners::codex_read_only(
                    &codex_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone()
                ),
            );
            let claude_out = claude_result?;
            let codex_out = codex_result?;

            // Union the missing lists from both agents
            let missing = union_missing(&claude_out, &codex_out);
            if missing.is_empty() {
                (true, String::new())
            } else {
                (false, missing.join(", "))
            }
        }

        // ── Security audit (multi-phase: OWASP + STRIDE + LLM security) ────
        "security" => {
            let prompt = match prompts {
                Some(p) => Prompts::render(&p.review_security, &[("task", task)]),
                None => format!(
                    "You are performing a security audit of this codebase.\n\
                     Task context: {task}\n\n\
                     Review files for: hardcoded secrets, XSS/injection/CSRF, insecure auth, log leaks.\n\
                     Write security.md with findings. Append [RESULT:PASS] or [RESULT:FAIL:reason]."
                ),
            };
            let prompt = super::inject_context(context, prompt);

            // Run Claude (deep audit) and Codex (independent cross-check) in parallel
            let codex_prompt = format!(
                "You are an independent security reviewer cross-checking a codebase.\n\
                 Task context: {task}\n\n\
                 Focus on these high-value checks only:\n\
                 1. Secrets in source code or .env files committed to repo\n\
                 2. SQL injection or command injection vectors\n\
                 3. Missing authentication/authorization on endpoints\n\
                 4. Unsafe deserialization of user input\n\
                 5. LLM prompt injection (if AI features exist)\n\n\
                 For each finding: [SEVERITY] file:line — description (confidence N/10)\n\
                 Only report findings with confidence >= 7.\n\n\
                 At the end: SECURITY_ISSUES:[count] or SECURITY_ISSUES:[0]"
            );
            let codex_prompt = super::inject_context(context, codex_prompt);

            let (claude_result, codex_result) = tokio::join!(
                runners::claude(&prompt, workspace, window_label, app_handle, token.clone()),
                runners::codex_read_only(
                    &codex_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone()
                ),
            );
            let claude_out = claude_result?;
            // Codex cross-check is best-effort; don't fail the phase if it errors
            let codex_extra = codex_result.unwrap_or_default();

            // If Codex found issues Claude missed, append them to the result
            let combined = if codex_extra.contains("SECURITY_ISSUES:[0]") || codex_extra.is_empty() {
                claude_out
            } else {
                format!(
                    "{claude_out}\n\n## Cross-Check Findings (Codex)\n\n{codex_extra}"
                )
            };
            parse_result(&combined)
        }

        // ── Specialist parallel review (security + performance + API + tests) ─
        "specialist_review" => {
            run_specialist_review(task, workspace, context, prompts, window_label, app_handle, token.clone()).await?
        }

        // ── Code cleanup — only files recorded in change.log ─────────────────
        "cleanup" => {
            // Read change.log to get the exact files Claude created/modified.
            // Deduplicate so each file is processed once.
            let change_log_content = workspace
                .map(|ws| std::fs::read_to_string(format!("{ws}/change.log")).unwrap_or_default())
                .unwrap_or_default();

            let file_list = build_cleanup_file_list(&change_log_content);

            if file_list.is_empty() {
                // No change.log or empty — emit PASS and return immediately
                app_handle
                    .emit_to(
                        EventTarget::webview_window(window_label),
                        "review-phase-result",
                        ReviewPhaseResult {
                            phase: phase.to_string(),
                            passed: true,
                            issue: String::new(),
                        },
                    )
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            let file_list_str = file_list.join("\n");
            let prompt = format!(
                "You are performing code cleanup on this codebase.\n\
                 Task context: {task}\n\n\
                 IMPORTANT: Only clean the files listed below — these are the files that were\n\
                 created or modified by the AI during this session (recorded in change.log).\n\
                 Do NOT touch any other file. User's pre-existing code must not be modified.\n\n\
                 Files to clean:\n\
                 {file_list_str}\n\n\
                 For EACH file in the list above:\n\
                 1. Read the file.\n\
                 2. Remove unused imports / use statements / require calls.\n\
                 3. Remove variables declared but never read.\n\
                 4. Remove functions defined in this file that are never called anywhere.\n\
                 5. Remove commented-out code blocks (not doc comments).\n\
                 6. Remove redundant debug print statements left in by mistake.\n\
                 7. Do NOT change any working logic or alter behaviour.\n\n\
                 Print one line per file:\n\
                 CLEANED <path> — <what was removed>\n\
                 SKIPPED <path> — already clean\n\n\
                 After all files, print a summary table:\n\
                 | File | Changes |\n\
                 |------|---------|\n\n\
                 At the very end append: [RESULT:PASS]"
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        // ── Design review — visual consistency and UI quality ────────────
        "design_review" => {
            let file_list = workspace
                .map(|ws| std::fs::read_to_string(format!("{ws}/change.log")).unwrap_or_default())
                .unwrap_or_default();
            let file_list = build_cleanup_file_list(&file_list);
            let ui_files: Vec<&str> = file_list
                .iter()
                .filter(|f| {
                    let f = f.to_lowercase();
                    f.ends_with(".tsx")
                        || f.ends_with(".jsx")
                        || f.ends_with(".vue")
                        || f.ends_with(".svelte")
                        || f.ends_with(".html")
                        || f.ends_with(".css")
                })
                .map(|s| s.as_str())
                .collect();

            if ui_files.is_empty() {
                app_handle
                    .emit_to(
                        EventTarget::webview_window(window_label),
                        "review-phase-result",
                        ReviewPhaseResult {
                            phase: phase.to_string(),
                            passed: true,
                            issue: String::new(),
                        },
                    )
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            let file_list_str = ui_files.join("\n");
            let prompt = format!(
                "You are a UI design reviewer checking visual quality and consistency.\n\
                 Task context: {task}\n\n\
                 Review ONLY these UI files:\n{file_list_str}\n\n\
                 For each file, check:\n\
                 1. **Spacing consistency** — 8px grid adherence, consistent padding within component types\n\
                 2. **Color palette** — max 5 colors per screen, no hardcoded hex (use theme tokens)\n\
                 3. **Typography** — max 3 font sizes per screen, proper hierarchy (bold headings, regular body)\n\
                 4. **Interactive states** — every clickable element has hover + focus states + cursor-pointer\n\
                 5. **Edge states** — loading skeleton, empty state with icon+message+CTA, error banner with retry\n\
                 6. **Responsive** — mobile-first, works at 375px/768px/1280px\n\
                 7. **Accessibility** — labels on inputs, alt on images, focus rings, sufficient contrast\n\n\
                 Report findings as:\n\
                 [SEVERITY:HIGH|MEDIUM|LOW] file:line — description\n\n\
                 At the end output: [RESULT:PASS] or [RESULT:FAIL:N design issues found]"
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        unknown => return Err(format!("Unknown review phase: {unknown}")),
    };

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "review-phase-result",
            ReviewPhaseResult {
                phase: phase.to_string(),
                passed,
                issue: found_issue.clone(),
            },
        )
        .map_err(|e| e.to_string())?;
    if let Some(workspace) = workspace {
        evidence::record_event(
            workspace,
            EvidenceEvent {
                ts: Utc::now().timestamp_millis() as u64,
                event_type: format!(
                    "review_{phase}_{}",
                    if passed { "passed" } else { "failed" }
                ),
                agent: "system".to_string(),
                subtask_id: None,
                summary: if found_issue.trim().is_empty() {
                    format!("Review phase {phase} completed successfully.")
                } else {
                    format!("Review phase {phase} completed with issue: {found_issue}")
                },
                artifacts: artifacts_for_review_phase(phase),
            },
        )?;
    }

    Ok(())
}

fn artifacts_for_review_phase(phase: &str) -> Vec<String> {
    match phase {
        "plan_check" => vec!["PLAN.md".to_string(), "BLACKBOARD.json".to_string()],
        "security" => vec!["security.md".to_string()],
        "specialist_review" => vec!["change.log".to_string()],
        "design_review" => vec!["change.log".to_string()],
        "cleanup" => vec!["change.log".to_string()],
        _ => Vec::new(),
    }
}

// ── Specialist parallel dispatch ─────────────────────────────────────────────

const SPECIALISTS: &[(&str, &str)] = &[
    ("security", "\
Check for:\n\
- SQL injection, command injection, path traversal\n\
- XSS and unsanitized user input in HTML output\n\
- Hardcoded secrets, API keys, tokens\n\
- Missing auth checks on protected endpoints\n\
- Insecure cryptography (MD5/SHA1 for passwords)\n\
- CSRF on state-mutating endpoints"),
    ("performance", "\
Check for:\n\
- N+1 query patterns in database access\n\
- Missing database indexes for frequent queries\n\
- Unbounded list/collection operations (no pagination)\n\
- Synchronous blocking calls in async code\n\
- Memory leaks (unclosed resources, growing caches)\n\
- Unnecessary re-renders in frontend components"),
    ("api_contract", "\
Check for:\n\
- Inconsistent API response formats (mixed error shapes)\n\
- Missing input validation on request bodies\n\
- Undocumented status codes or error cases\n\
- Breaking changes vs existing API consumers\n\
- Missing Content-Type headers or wrong MIME types\n\
- Enum/union types not exhaustively handled"),
    ("testing", "\
Check for:\n\
- Critical paths without test coverage (auth, payment, data mutation)\n\
- Tests that always pass (no meaningful assertions)\n\
- Missing edge case tests (empty input, boundary values, error paths)\n\
- Flaky test patterns (timing-dependent, order-dependent)\n\
- Missing integration tests for cross-module interactions\n\
- Test files that import but don't test the changed code"),
];

#[allow(clippy::too_many_arguments)]
async fn run_specialist_review(
    task: &str,
    workspace: Option<&str>,
    context: Option<&str>,
    prompts: Option<&Prompts>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(bool, String), String> {
    // Build specialist prompts
    let specialist_prompts: Vec<(String, String)> = SPECIALISTS
        .iter()
        .map(|(name, instructions)| {
            let prompt = match prompts {
                Some(p) => Prompts::render(
                    &p.review_specialist,
                    &[
                        ("task", task),
                        ("specialty", name),
                        ("specialty_instructions", instructions),
                    ],
                ),
                None => format!(
                    "You are a {name} specialist reviewing code.\n\
                     Task: {task}\n\n{instructions}\n\n\
                     Report findings as: [SEVERITY] file:line — description\n\
                     End with: SPECIALIST_VERDICT:PASS or SPECIALIST_VERDICT:FAIL:<count> findings"
                ),
            };
            (name.to_string(), super::inject_context(context, prompt))
        })
        .collect();

    // Run all specialists in parallel using Codex (read-only, fast)
    let mut join_set = tokio::task::JoinSet::new();
    for (name, prompt) in specialist_prompts {
        let ws = workspace.map(ToOwned::to_owned);
        let wl = window_label.to_string();
        let ah = app_handle.clone();
        let tk = token.clone();
        join_set.spawn(async move {
            let result = runners::codex_read_only_quiet(
                &prompt,
                ws.as_deref(),
                &wl,
                &ah,
                tk,
            )
            .await;
            (name, result)
        });
    }

    let mut all_passed = true;
    let mut issues = Vec::new();

    while let Some(joined) = join_set.join_next().await {
        let (name, result) = joined.map_err(|e| format!("Specialist worker crashed: {e}"))?;
        match result {
            Ok(output) => {
                if output.contains("SPECIALIST_VERDICT:FAIL") {
                    all_passed = false;
                    // Extract the summary after FAIL:
                    if let Some(rest) = output.rsplit("SPECIALIST_VERDICT:FAIL:").next() {
                        let summary = rest.lines().next().unwrap_or("issues found").trim();
                        issues.push(format!("{name}: {summary}"));
                    } else {
                        issues.push(format!("{name}: issues found"));
                    }
                }
            }
            Err(err) => {
                // Specialist failure counts as not-passed — log and fail
                all_passed = false;
                issues.push(format!("{name}: specialist error — {err}"));
            }
        }
    }

    if all_passed {
        Ok((true, String::new()))
    } else {
        Ok((false, issues.join("; ")))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the MISSING:[...] list from one agent's output.
/// Returns a Vec of trimmed item strings, empty if nothing is missing.
fn extract_missing(text: &str) -> Vec<String> {
    // Find the last line starting with "MISSING:[" to avoid false positives
    for line in text.lines().rev() {
        let t = line.trim();
        if let Some(inner) = t.strip_prefix("MISSING:[") {
            let inner = inner.trim_end_matches(']').trim();
            if inner.is_empty() {
                return vec![];
            }
            return inner
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    vec![]
}

/// Union the MISSING lists from Claude and Codex, deduplicating by ID prefix (e.g. "F2", "P3").
fn union_missing(claude_out: &str, codex_out: &str) -> Vec<String> {
    let mut claude_list = extract_missing(claude_out);
    let codex_list = extract_missing(codex_out);

    // Deduplicate: add codex items whose ID is not already present from Claude
    for item in codex_list {
        let id = item.split_whitespace().next().unwrap_or("").to_string();
        let already = claude_list
            .iter()
            .any(|existing| existing.split_whitespace().next().unwrap_or("") == id);
        if !already {
            claude_list.push(item);
        }
    }
    claude_list
}

/// Parse change.log content into a deduplicated list of absolute file paths.
/// Each line is expected to be "CREATE: <path>" or "MODIFY: <path>".
/// Non-existent paths are filtered out.
fn build_cleanup_file_list(change_log: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for line in change_log.lines() {
        let path = line
            .strip_prefix("CREATE: ")
            .or_else(|| line.strip_prefix("MODIFY: "))
            .map(|s| s.trim().to_string());
        if let Some(p) = path {
            if !p.is_empty() && seen.insert(p.clone()) && std::path::Path::new(&p).exists() {
                result.push(p);
            }
        }
    }
    result
}

/// Parse the `[RESULT:PASS]` / `[RESULT:FAIL:reason]` marker appended at the
/// end of a Claude review output.
/// Fail-closed: no explicit [RESULT:PASS] marker → treated as failure so that
/// truncated output or missing markers never silently appear as success.
fn parse_result(text: &str) -> (bool, String) {
    if let Some(pos) = text.rfind("[RESULT:") {
        let suffix = &text[pos..];
        if suffix.starts_with("[RESULT:PASS]") {
            return (true, String::new());
        }
        if suffix.starts_with("[RESULT:FAIL:") {
            let issue = suffix
                .trim_start_matches("[RESULT:FAIL:")
                .splitn(2, ']')
                .next()
                .unwrap_or("unknown issue")
                .to_string();
            return (false, issue);
        }
        // [RESULT:...] found but neither PASS nor FAIL:reason — treat as failure
        return (
            false,
            format!(
                "malformed result marker: {}",
                &suffix[..suffix.len().min(40)]
            ),
        );
    }
    // No marker at all — output was likely truncated or agent skipped it
    (
        false,
        "no [RESULT:*] marker found — output may be truncated".to_string(),
    )
}
