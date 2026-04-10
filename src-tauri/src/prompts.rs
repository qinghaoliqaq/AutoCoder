/// Prompt loader.
///
/// Loading order (first match wins):
///   1. <executable dir>/prompts/<name>.md   — production override
///   2. <cwd>/prompts/<name>.md              — dev override
///   3. <cwd>/src-tauri/prompts/<name>.md    — in-tree dev (cargo run from project root)
///   4. Compiled-in default via include_str! — always available, no file I/O needed
use std::path::PathBuf;

// ── Compiled-in defaults (always available) ───────────────────────────────────

const DEFAULT_DIRECTOR_CHAT: &str = include_str!("../prompts/director_chat.md");
const DEFAULT_PLAN_CLAUDE: &str = include_str!("../prompts/plan_claude.md");
const DEFAULT_PLAN_CODEX: &str = include_str!("../prompts/plan_codex.md");
const DEFAULT_PLAN_CLAUDE_RESPONSE: &str = include_str!("../prompts/plan_claude_response.md");
const DEFAULT_PLAN_CODEX_FINAL: &str = include_str!("../prompts/plan_codex_final.md");
const DEFAULT_PLAN_DIRECTOR_VERDICT: &str = include_str!("../prompts/plan_director_verdict.md");
const DEFAULT_PLAN_NAME: &str = include_str!("../prompts/plan_name.md");
const DEFAULT_PLAN_SYNTHESIS: &str = include_str!("../prompts/plan_synthesis.md");
const DEFAULT_PLAN_REVIEW_CLAUDE: &str = include_str!("../prompts/plan_review_claude.md");
const DEFAULT_PLAN_REVIEW_CODEX: &str = include_str!("../prompts/plan_review_codex.md");
const DEFAULT_PLAN_REVIEW_CLAUDE_RESP: &str =
    include_str!("../prompts/plan_review_claude_response.md");
const DEFAULT_PLAN_REVIEW_CODEX_FINAL: &str = include_str!("../prompts/plan_review_codex_final.md");
const DEFAULT_PLAN_REVIEW_SYNTHESIS: &str = include_str!("../prompts/plan_review_synthesis.md");
const DEFAULT_PLAN_REVIEW_CODEX_PARALLEL: &str =
    include_str!("../prompts/plan_review_codex_parallel.md");
const DEFAULT_CODE_CLAUDE: &str = include_str!("../prompts/code_claude.md");
const DEFAULT_DEBUG_CLAUDE: &str = include_str!("../prompts/debug_claude.md");
const DEFAULT_DEBUG_CODEX: &str = include_str!("../prompts/debug_codex.md");
const DEFAULT_TEST_CLAUDE: &str = include_str!("../prompts/test_claude.md");
const DEFAULT_COMPACT_SUMMARY: &str = include_str!("../prompts/compact_summary.md");
const DEFAULT_REVIEW_SECURITY: &str = include_str!("../prompts/review_security.md");
const DEFAULT_REVIEW_SPECIALIST: &str = include_str!("../prompts/review_specialist.md");

// ── Public struct ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Prompts {
    pub director_chat: String,
    pub plan_claude: String,
    pub plan_codex: String,
    pub plan_claude_response: String,
    pub plan_codex_final: String,
    pub plan_director_verdict: String,
    pub plan_name: String,
    pub plan_synthesis: String,
    pub plan_review_claude: String,
    pub plan_review_codex: String,
    pub plan_review_claude_resp: String,
    pub plan_review_codex_final: String,
    pub plan_review_synthesis: String,
    pub plan_review_codex_parallel: String,
    pub code_claude: String,
    pub debug_claude: String,
    pub debug_codex: String,
    pub test_claude: String,
    pub compact_summary: String,
    pub review_security: String,
    pub review_specialist: String,
}

impl Prompts {
    pub fn load() -> Self {
        let search_dirs = runtime_search_dirs();

        Self {
            director_chat: load("director_chat.md", &search_dirs, DEFAULT_DIRECTOR_CHAT),
            plan_claude: load("plan_claude.md", &search_dirs, DEFAULT_PLAN_CLAUDE),
            plan_codex: load("plan_codex.md", &search_dirs, DEFAULT_PLAN_CODEX),
            plan_claude_response: load(
                "plan_claude_response.md",
                &search_dirs,
                DEFAULT_PLAN_CLAUDE_RESPONSE,
            ),
            plan_codex_final: load(
                "plan_codex_final.md",
                &search_dirs,
                DEFAULT_PLAN_CODEX_FINAL,
            ),
            plan_director_verdict: load(
                "plan_director_verdict.md",
                &search_dirs,
                DEFAULT_PLAN_DIRECTOR_VERDICT,
            ),
            plan_name: load("plan_name.md", &search_dirs, DEFAULT_PLAN_NAME),
            plan_synthesis: load("plan_synthesis.md", &search_dirs, DEFAULT_PLAN_SYNTHESIS),
            plan_review_claude: load(
                "plan_review_claude.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_CLAUDE,
            ),
            plan_review_codex: load(
                "plan_review_codex.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_CODEX,
            ),
            plan_review_claude_resp: load(
                "plan_review_claude_response.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_CLAUDE_RESP,
            ),
            plan_review_codex_final: load(
                "plan_review_codex_final.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_CODEX_FINAL,
            ),
            plan_review_synthesis: load(
                "plan_review_synthesis.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_SYNTHESIS,
            ),
            plan_review_codex_parallel: load(
                "plan_review_codex_parallel.md",
                &search_dirs,
                DEFAULT_PLAN_REVIEW_CODEX_PARALLEL,
            ),
            code_claude: load("code_claude.md", &search_dirs, DEFAULT_CODE_CLAUDE),
            debug_claude: load("debug_claude.md", &search_dirs, DEFAULT_DEBUG_CLAUDE),
            debug_codex: load("debug_codex.md", &search_dirs, DEFAULT_DEBUG_CODEX),
            test_claude: load("test_claude.md", &search_dirs, DEFAULT_TEST_CLAUDE),
            compact_summary: load("compact_summary.md", &search_dirs, DEFAULT_COMPACT_SUMMARY),
            review_security: load("review_security.md", &search_dirs, DEFAULT_REVIEW_SECURITY),
            review_specialist: load(
                "review_specialist.md",
                &search_dirs,
                DEFAULT_REVIEW_SPECIALIST,
            ),
        }
    }

    /// Fill `{{variable}}` placeholders in a prompt template.
    ///
    /// Uses a single-pass scan to prevent template injection: if a *value*
    /// itself contains `{{another_key}}`, it will NOT be expanded.
    /// This matters because `task` is user-supplied text.
    pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
        use std::collections::HashMap;
        let map: HashMap<&str, &str> = vars.iter().copied().collect();
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after_open = &rest[start + 2..];
            if let Some(end) = after_open.find("}}") {
                let key = &after_open[..end];
                if let Some(&value) = map.get(key) {
                    out.push_str(value);
                } else {
                    // Unknown placeholder — keep it verbatim
                    out.push_str(&rest[start..start + 2 + end + 2]);
                }
                rest = &after_open[end + 2..];
            } else {
                // No closing "}}" — emit the rest as-is
                out.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
        out.push_str(rest);
        out
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load(filename: &str, search_dirs: &[PathBuf], default: &str) -> String {
    for dir in search_dirs {
        let path = dir.join("prompts").join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !content.trim().is_empty() {
                return content;
            }
        }
    }
    default.to_string()
}

fn runtime_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // 1. next to the compiled binary (production)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }

    // 2. current working directory (development: cargo run)
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.clone());
        // 3. src-tauri/ sub-directory (when cwd is the project root)
        dirs.push(cwd.join("src-tauri"));
    }

    dirs
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_single_var() {
        let tmpl = "Hello {{name}}!";
        assert_eq!(Prompts::render(tmpl, &[("name", "World")]), "Hello World!");
    }

    #[test]
    fn render_substitutes_multiple_vars() {
        let tmpl = "{{a}} + {{b}} = {{c}}";
        let result = Prompts::render(tmpl, &[("a", "1"), ("b", "2"), ("c", "3")]);
        assert_eq!(result, "1 + 2 = 3");
    }

    #[test]
    fn render_leaves_unknown_placeholders_intact() {
        let tmpl = "Hello {{name}} {{surname}}";
        assert_eq!(
            Prompts::render(tmpl, &[("name", "Alice")]),
            "Hello Alice {{surname}}"
        );
    }

    #[test]
    fn render_noop_on_template_without_placeholders() {
        let tmpl = "No placeholders here.";
        assert_eq!(Prompts::render(tmpl, &[("x", "y")]), tmpl);
    }

    #[test]
    fn render_handles_empty_value() {
        let tmpl = "prefix-{{x}}-suffix";
        assert_eq!(Prompts::render(tmpl, &[("x", "")]), "prefix--suffix");
    }

    #[test]
    fn render_handles_value_containing_braces() {
        let tmpl = "val={{v}}";
        assert_eq!(Prompts::render(tmpl, &[("v", "{raw}")]), "val={raw}");
    }
}
