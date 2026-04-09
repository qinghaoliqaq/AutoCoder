/// Prompt builders and output parsers for the code skill.
///
/// All functions here are pure string manipulation — no async, no I/O,
/// no Tauri dependencies.  They construct the prompts sent to Claude/Codex
/// and parse the structured markers they return.
use super::blackboard::{SubtaskCard, BLACKBOARD_MD};
use super::vendored::VendoredSkill;
use super::planning_schema::SubtaskAcceptance;
use super::verifier::VERIFIER_RESULT_JSON;

// ── Prompt builders ───────────────────────────────────────────────────────

pub(super) fn build_implement_prompt(
    base_prompt: &str,
    task: &str,
    card: &SubtaskCard,
    acceptance: Option<&SubtaskAcceptance>,
    vendored_skill: Option<&VendoredSkill>,
) -> String {
    format!(
        "{base_prompt}\n\n\
Shared-blackboard contract:\n\
- Read .ai-dev-hub/PLAN.md and {BLACKBOARD_MD} in the current directory before making changes.\n\
- Do not rely on direct agent-to-agent conversation.\n\
- Do not implement the whole project in one pass; focus only on the current subtask.\n\
- You may touch shared code if required, but only to complete this subtask cleanly.\n\
- Work only inside the isolated workspace you were given for this subtask.\n\
- The isolated workspace does NOT include node_modules, target, dist, or other build artifacts. If you add or change dependencies in package.json (or Cargo.toml, etc.), run the appropriate install command (npm install / pnpm install / cargo build) so that subsequent build checks can pass.\n\
- If packaged vendored skill guidance conflicts with .ai-dev-hub/PLAN.md, .ai-dev-hub/PLAN_BLACKBOARD.md, {BLACKBOARD_MD}, or the current subtask contract, follow the local project rules.\n\
\n\
Current task context: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
\n\
{acceptance_block}\n\
\n\
{vendored_block}\n\
\n\
Completion rule:\n\
- Finish only when this subtask is fully implemented and ready for Codex review.\n\
- Keep your response concise.\n\
\n\
At the very end output exactly these lines:\n\
SUBTASK_ID: {id}\n\
IMPLEMENTATION_SUMMARY: <one concise paragraph>\n\
FILES_TOUCHED: <comma-separated relative paths or none>",
        id = card.id,
        title = card.title,
        description = card.description,
        acceptance_block = render_acceptance_block(acceptance),
        vendored_block = render_vendored_block(vendored_skill),
    )
}

pub(super) fn build_fix_prompt(
    base_prompt: &str,
    task: &str,
    card: &SubtaskCard,
    acceptance: Option<&SubtaskAcceptance>,
    vendored_skill: Option<&VendoredSkill>,
) -> String {
    let findings = if card.review_findings.is_empty() {
        "- none".to_string()
    } else {
        card.review_findings
            .iter()
            .map(|finding| format!("- {finding}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Build optional context sections that only appear when relevant.
    let mut extra_sections = Vec::new();

    // Prior files touched — tells Claude which files to focus on.
    if !card.files_touched.is_empty() {
        extra_sections.push(format!(
            "Files modified in previous attempt (focus your fixes here):\n{}",
            card.files_touched
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    // Merge conflict context — critical info that was previously lost.
    if let Some(conflict) = &card.merge_conflict {
        extra_sections.push(format!(
            "Merge conflict from previous attempt (your fix must resolve this):\n{conflict}"
        ));
    }

    // Prior attempted fixes — prevents Claude from repeating failed approaches.
    if !card.attempted_fixes.is_empty() {
        extra_sections.push(format!(
            "Previously attempted approaches that FAILED (do NOT repeat these):\n{}",
            card.attempted_fixes
                .iter()
                .map(|a| format!("- {a}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let extra = if extra_sections.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", extra_sections.join("\n\n"))
    };

    format!(
        "{base_prompt}\n\n\
Shared-blackboard contract:\n\
- Read .ai-dev-hub/PLAN.md and {BLACKBOARD_MD} in the current directory before making changes.\n\
- Treat the review findings below as the only coordination channel from Codex.\n\
- Fix the current subtask; do not drift into unrelated features.\n\
- Work only inside the isolated workspace you were given for this subtask.\n\
- The isolated workspace does NOT include node_modules, target, dist, or other build artifacts. If you add or change dependencies in package.json (or Cargo.toml, etc.), run the appropriate install command (npm install / pnpm install / cargo build) so that subsequent build checks can pass.\n\
- If packaged vendored skill guidance conflicts with .ai-dev-hub/PLAN.md, .ai-dev-hub/PLAN_BLACKBOARD.md, {BLACKBOARD_MD}, or the current subtask contract, follow the local project rules.\n\
\n\
Current task context: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
\n\
{acceptance_block}\n\
\n\
{vendored_block}\n\
\n\
Blackboard review findings to resolve:\n\
{findings}\n\
{extra}\n\
CRITICAL — Fix strategy (do NOT rewrite from scratch):\n\
- Your workspace already contains the code from the PREVIOUS attempt. Do NOT delete or rewrite it wholesale.\n\
- Read the existing files first, then make TARGETED edits to address each finding below.\n\
- Address EACH finding above as a checklist item — do not skip any.\n\
- If compile or type-check errors are listed, fix every error so the build passes cleanly.\n\
- If a merge conflict is noted, restructure your changes to avoid conflicting with parallel subtask output.\n\
- If previous approaches failed, try a fundamentally different strategy — but still start from the current code.\n\
\n\
At the very end output exactly these lines:\n\
SUBTASK_ID: {id}\n\
IMPLEMENTATION_SUMMARY: <one concise paragraph about the fixes>\n\
FILES_TOUCHED: <comma-separated relative paths or none>",
        id = card.id,
        title = card.title,
        description = card.description,
        acceptance_block = render_acceptance_block(acceptance),
        vendored_block = render_vendored_block(vendored_skill),
    )
}

pub(super) fn build_review_prompt(
    task: &str,
    card: &SubtaskCard,
    acceptance: Option<&SubtaskAcceptance>,
    verifier_warnings: &[String],
) -> String {
    let files = if card.files_touched.is_empty() {
        "none".to_string()
    } else {
        card.files_touched.join(", ")
    };

    let verifier_section = if verifier_warnings.is_empty() {
        "Verifier: passed with no warnings.".to_string()
    } else {
        format!(
            "Verifier passed but flagged these warnings (confirm or escalate):\n{}",
            verifier_warnings
                .iter()
                .map(|w| format!("- {w}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        "You are Codex reviewing exactly one implementation subtask.\n\n\
Shared-blackboard contract:\n\
- Read .ai-dev-hub/PLAN.md and {BLACKBOARD_MD} from the current directory before reviewing.\n\
- Read {VERIFIER_RESULT_JSON} from the current directory before reviewing.\n\
- Do not rely on direct Claude transcript as the source of truth.\n\
- Do not edit files. Your job is review only.\n\
- Review only the current subtask, but verify integration points it depends on.\n\
- The implementation was done in an isolated workspace; if it passes, it will merge back into the main workspace afterwards.\n\
\n\
Overall task: {task}\n\
Current subtask:\n\
- ID: {id}\n\
- Title: {title}\n\
- Description: {description}\n\
- Files recently touched: {files}\n\
- Blackboard implementation summary: {implementation}\n\
\n\
{verifier_section}\n\
\n\
{acceptance_block}\n\
\n\
Review standard:\n\
- PASS if the subtask is implemented correctly and completely, meets all acceptance criteria, and has no correctness or quality gaps.\n\
- PASS if the implementation satisfies the structured acceptance requirements below when they are provided.\n\
- FAIL if required behavior is missing, partially implemented, or incorrectly wired.\n\
- FAIL if there is a runtime crash/error, data loss risk, or a security vulnerability.\n\
- FAIL if the code has clear quality issues: wrong component usage, missing error handling where needed, broken UI/UX patterns, incorrect types or props.\n\
- If the verifier flagged warnings above, confirm whether they are actual issues or false positives.\n\
\n\
At the very end output exactly this shape:\n\
REVIEW_DECISION: PASS or FAIL\n\
REVIEW_SUMMARY: <one concise paragraph>\n\
REVIEW_FINDINGS:\n\
- <specific actionable issue>\n\
\n\
If there are no issues, write:\n\
REVIEW_FINDINGS:\n\
- none",
        id = card.id,
        title = card.title,
        description = card.description,
        implementation = card.latest_implementation.as_deref().unwrap_or("none"),
        acceptance_block = render_acceptance_block(acceptance),
    )
}

// ── Acceptance / vendored rendering ───────────────────────────────────────

pub(super) fn render_acceptance_block(acceptance: Option<&SubtaskAcceptance>) -> String {
    let Some(acceptance) = acceptance else {
        return "Structured acceptance for this subtask: none provided.".to_string();
    };

    let must_have = render_acceptance_list(&acceptance.must_have);
    let must_not = render_acceptance_list(&acceptance.must_not);
    let evidence_required = render_acceptance_list(&acceptance.evidence_required);
    let qa_focus = render_acceptance_list(&acceptance.qa_focus);

    format!(
        "Structured acceptance for this subtask:\n\
- must_have:\n{must_have}\n\
- must_not:\n{must_not}\n\
- evidence_required:\n{evidence_required}\n\
- qa_focus:\n{qa_focus}"
    )
}

fn render_acceptance_list(items: &[String]) -> String {
    if items.is_empty() {
        "  - none".to_string()
    } else {
        items
            .iter()
            .map(|item| format!("  - {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(super) fn render_vendored_block(vendored_skill: Option<&VendoredSkill>) -> String {
    let Some(skill) = vendored_skill else {
        return "Packaged vendored skill: none for this subtask.".to_string();
    };

    format!(
        "Bundled skill available:\n\
- ID: {id}\n\
- Label: {label}\n\
- Follow the skill instructions below when implementing this subtask.\n\
\n\
Skill instructions:\n\
```markdown\n\
{excerpt}\n\
```",
        id = skill.id.slug(),
        label = skill.id.label(),
        excerpt = skill.excerpt
    )
}

// ── Output parsers ────────────────────────────────────────────────────────

pub(super) struct ImplementationReport {
    pub summary: String,
    pub files_touched: Vec<String>,
}

pub(super) fn parse_implementation_report(
    output: &str,
    observed_files: &[String],
    subtask_id: &str,
) -> ImplementationReport {
    let summary = extract_marker_line(output, "IMPLEMENTATION_SUMMARY:")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            fallback_summary(output, &format!("Implementation finished for {subtask_id}"))
        });

    let marker_files = extract_marker_line(output, "FILES_TOUCHED:")
        .map(|line| split_csv(&line))
        .unwrap_or_default();

    let files_touched = if !marker_files.is_empty() && marker_files != ["none".to_string()] {
        marker_files
    } else {
        observed_files.to_vec()
    };

    ImplementationReport {
        summary,
        files_touched,
    }
}

pub(super) struct ReviewReport {
    pub passed: bool,
    pub summary: String,
    pub findings: Vec<String>,
}

pub(super) fn parse_review_report(output: &str) -> ReviewReport {
    let explicit_decision = extract_marker_line(output, "REVIEW_DECISION:");

    let summary = extract_marker_line(output, "REVIEW_SUMMARY:")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_summary(output, "Codex review completed"));

    let findings = extract_list_after_header(output, "REVIEW_FINDINGS:");
    let findings = if findings.len() == 1 && findings[0].eq_ignore_ascii_case("none") {
        Vec::new()
    } else {
        findings
    };

    let passed = match &explicit_decision {
        Some(decision) => decision.to_uppercase().contains("PASS"),
        None => {
            // No explicit decision marker — infer from output content.
            // Consider both signal words AND whether actionable findings exist.
            infer_review_passed(output, &findings)
        }
    };

    ReviewReport {
        passed,
        summary,
        findings,
    }
}

/// When the explicit REVIEW_DECISION marker is missing, try to infer the
/// result from the review body.  We look for strong positive language that
/// indicates approval and check for the absence of blocking language.
///
/// The parsed `findings` list is used as a tiebreaker: if there are no
/// signal words in either direction AND no actionable findings, the review
/// is treated as a pass.  This prevents the "fail with nothing to fix"
/// loop that wastes all retry attempts.
fn infer_review_passed(output: &str, findings: &[String]) -> bool {
    let upper = output.to_uppercase();

    // Check for SPECIALIST_VERDICT which uses a different format.
    if upper.contains("SPECIALIST_VERDICT:PASS") {
        return true;
    }
    if upper.contains("SPECIALIST_VERDICT:FAIL") {
        return false;
    }

    // Strong pass signals.
    let pass_signals = [
        "LGTM",
        "LOOKS GOOD",
        "PASSES REVIEW",
        "PASSES THE REVIEW",
        "REVIEW: PASS",
        "DECISION: PASS",
        "VERDICT: PASS",
        "APPROVED",
        "ALL CHECKS PASS",
        "NO CRITICAL",
        "NO HIGH OR CRITICAL",
        "NO BLOCKING",
    ];
    let has_pass_signal = pass_signals.iter().any(|s| upper.contains(s));

    // Strong fail signals.
    let fail_signals = [
        "REVIEW: FAIL",
        "DECISION: FAIL",
        "VERDICT: FAIL",
        "CRITICAL FINDING",
        "BLOCKS MERGE",
        "MUST FIX",
        "CANNOT PASS",
        "REJECTED",
    ];
    let has_fail_signal = fail_signals.iter().any(|s| upper.contains(s));

    if has_fail_signal {
        return false;
    }
    if has_pass_signal {
        return true;
    }

    // No strong signal in either direction.  Use findings as tiebreaker:
    // if the reviewer listed no actionable findings, treat as pass.
    // This prevents "fail with nothing to fix" retry loops.
    findings.is_empty()
}

// ── String helpers ────────────────────────────────────────────────────────

fn extract_marker_line(output: &str, prefix: &str) -> Option<String> {
    output.lines().rev().find_map(|line| {
        line.trim()
            .strip_prefix(prefix)
            .map(|s| s.trim().to_string())
    })
}

fn extract_list_after_header(output: &str, header: &str) -> Vec<String> {
    let mut collecting = false;
    let mut items = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            collecting = true;
            continue;
        }
        if collecting {
            if let Some(item) = trimmed.strip_prefix("- ") {
                items.push(item.trim().to_string());
                continue;
            }
            if !items.is_empty() && !trimmed.is_empty() {
                break;
            }
        }
    }

    items
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn fallback_summary(output: &str, fallback: &str) -> String {
    let output = output.trim();
    if output.is_empty() {
        return fallback.to_string();
    }

    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(220).collect())
        .unwrap_or_else(|| fallback.to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_acceptance_block_includes_structured_lists() {
        let acceptance = SubtaskAcceptance {
            subtask_id: "F1".to_string(),
            must_have: vec!["Create job".to_string()],
            must_not: vec!["Return 500 on validation".to_string()],
            evidence_required: vec!["API tests".to_string()],
            qa_focus: vec!["Validation rules".to_string()],
        };

        let rendered = render_acceptance_block(Some(&acceptance));
        assert!(rendered.contains("must_have"));
        assert!(rendered.contains("Create job"));
        assert!(rendered.contains("must_not"));
        assert!(rendered.contains("Return 500 on validation"));
        assert!(rendered.contains("evidence_required"));
        assert!(rendered.contains("API tests"));
        assert!(rendered.contains("qa_focus"));
        assert!(rendered.contains("Validation rules"));
    }

    #[test]
    fn render_acceptance_block_handles_missing_acceptance() {
        assert_eq!(
            render_acceptance_block(None),
            "Structured acceptance for this subtask: none provided."
        );
    }

    fn fix_prompt_card() -> SubtaskCard {
        use crate::skills::blackboard::{SubtaskKind, SubtaskState};
        SubtaskCard {
            id: "F1".to_string(),
            title: "Jobs API".to_string(),
            description: "Build job routes".to_string(),
            kind: SubtaskKind::Feature,
            depends_on: Vec::new(),
            can_run_in_parallel: true,
            parallel_group: None,
            suggested_skill: None,
            expected_touch: Vec::new(),
            status: SubtaskState::NeedsFix,
            attempts: 2,
            latest_implementation: Some("Added CRUD endpoints".to_string()),
            latest_review: Some("Missing validation".to_string()),
            review_findings: vec!["Input validation missing on POST /jobs".to_string()],
            files_touched: vec![
                "src/jobs/api.rs".to_string(),
                "src/jobs/model.rs".to_string(),
            ],
            isolated_workspace: None,
            merge_conflict: None,
            attempted_fixes: Vec::new(),
        }
    }

    #[test]
    fn build_fix_prompt_includes_focus_files() {
        let card = fix_prompt_card();
        let prompt = build_fix_prompt("base", "task", &card, None, None);
        assert!(prompt.contains("Files modified in previous attempt"));
        assert!(prompt.contains("src/jobs/api.rs"));
        assert!(prompt.contains("src/jobs/model.rs"));
    }

    #[test]
    fn build_fix_prompt_includes_merge_conflict() {
        let mut card = fix_prompt_card();
        card.merge_conflict = Some("Conflict in src/shared/db.rs between F1 and F2".to_string());
        let prompt = build_fix_prompt("base", "task", &card, None, None);
        assert!(prompt.contains("Merge conflict from previous attempt"));
        assert!(prompt.contains("Conflict in src/shared/db.rs between F1 and F2"));
    }

    #[test]
    fn build_fix_prompt_includes_attempted_fixes() {
        let mut card = fix_prompt_card();
        card.attempted_fixes = vec!["Attempt 1: Added basic CRUD without validation".to_string()];
        let prompt = build_fix_prompt("base", "task", &card, None, None);
        assert!(prompt.contains("Previously attempted approaches that FAILED"));
        assert!(prompt.contains("Attempt 1: Added basic CRUD without validation"));
    }

    #[test]
    fn build_fix_prompt_omits_empty_sections() {
        let mut card = fix_prompt_card();
        card.files_touched.clear();
        card.merge_conflict = None;
        card.attempted_fixes.clear();
        let prompt = build_fix_prompt("base", "task", &card, None, None);
        assert!(!prompt.contains("Files modified in previous attempt"));
        assert!(!prompt.contains("Merge conflict from previous attempt"));
        assert!(!prompt.contains("Previously attempted approaches"));
    }

    #[test]
    fn build_review_prompt_includes_verifier_warnings() {
        let card = fix_prompt_card();
        let warnings = vec!["Touched files outside expected scope: src/auth/mod.rs".to_string()];
        let prompt = build_review_prompt("task", &card, None, &warnings);
        assert!(prompt.contains("Verifier passed but flagged these warnings"));
        assert!(prompt.contains("Touched files outside expected scope"));
        assert!(prompt.contains("confirm or escalate"));
    }

    #[test]
    fn build_review_prompt_clean_verifier_pass() {
        let card = fix_prompt_card();
        let prompt = build_review_prompt("task", &card, None, &[]);
        assert!(prompt.contains("Verifier: passed with no warnings."));
    }

    #[test]
    fn parse_review_report_explicit_pass() {
        let output = "All good.\nREVIEW_DECISION: PASS\nREVIEW_SUMMARY: Looks correct\nREVIEW_FINDINGS:\n- none\n";
        let report = parse_review_report(output);
        assert!(report.passed);
        assert_eq!(report.summary, "Looks correct");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn parse_review_report_explicit_fail() {
        let output = "Problems found.\nREVIEW_DECISION: FAIL\nREVIEW_SUMMARY: Missing validation\nREVIEW_FINDINGS:\n- No input sanitization\n";
        let report = parse_review_report(output);
        assert!(!report.passed);
        assert_eq!(report.findings, vec!["No input sanitization"]);
    }

    #[test]
    fn parse_review_report_no_marker_with_pass_signal() {
        let output = "The implementation looks good. All checks pass and there are no blocking issues. LGTM.\n";
        let report = parse_review_report(output);
        assert!(report.passed);
    }

    #[test]
    fn parse_review_report_no_marker_with_fail_signal() {
        let output =
            "Critical finding: the API endpoint has no authentication.\nMust fix before merging.\n";
        let report = parse_review_report(output);
        assert!(!report.passed);
    }

    #[test]
    fn parse_review_report_no_marker_no_signal_no_findings_defaults_to_pass() {
        // No signal words, no findings → treat as pass to avoid
        // "fail with nothing to fix" retry loops.
        let output = "Some random text without any clear verdict.\n";
        let report = parse_review_report(output);
        assert!(report.passed);
    }

    #[test]
    fn parse_review_report_no_marker_no_signal_with_findings_defaults_to_fail() {
        // No signal words but has actionable findings → fail.
        let output = "The code has issues.\nREVIEW_FINDINGS:\n- Missing error handling in auth module\n";
        let report = parse_review_report(output);
        assert!(!report.passed);
        assert_eq!(report.findings, vec!["Missing error handling in auth module"]);
    }

    #[test]
    fn parse_review_report_specialist_verdict_pass() {
        let output = "No HIGH or CRITICAL findings.\nSPECIALIST_VERDICT:PASS — no issues found\n";
        let report = parse_review_report(output);
        assert!(report.passed);
    }

    #[test]
    fn parse_review_report_specialist_verdict_fail() {
        let output = "Found 2 critical issues.\nSPECIALIST_VERDICT:FAIL:2 HIGH/CRITICAL findings — SQL injection\n";
        let report = parse_review_report(output);
        assert!(!report.passed);
    }
}
