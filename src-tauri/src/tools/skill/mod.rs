pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};
use crate::bundled_skills::{ParsedSkill, SkillProvider, SkillRegistry};

/// Skill tool — executes a markdown skill (Warp/Claude/Codex-compatible) by name.
///
/// Skills are prompt-based contextual guides that provide domain-specific
/// instructions for the coding agent. When invoked, the skill's full body is
/// returned so the model can follow the specialized instructions.
///
/// Discovery is workspace-aware: project skills under
/// `<workspace>/.agents/skills/` override user-level skills, which override
/// Claude/Codex skill directories, which override the compiled-in builtins.
pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &'static str {
        "Skill"
    }

    fn description(&self) -> &'static str {
        "Execute a skill within the main conversation. When users ask you to \
         perform tasks, check if any of the available skills match. Skills provide \
         specialized capabilities and domain knowledge. When users reference a \
         \"slash command\" or \"/<something>\" (e.g., \"/simplify\", \"/verify\"), \
         they are referring to a skill. Use this tool to invoke it."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["skill"],
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"simplify\", \"verify\", \"frontend-dev\""
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        // SkillTool.execute only resolves a skill by name and returns its
        // markdown body — no filesystem writes, no shell, no network.
        // Follow-up tool calls the model makes based on that body are gated
        // by their own is_read_only checks.
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let skill_name = match input["skill"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::err("Missing required parameter: skill"),
        };

        let registry = SkillRegistry::discover(Some(ctx.workspace));
        match registry.resolve(skill_name) {
            Some(def) => {
                let args_note = match input["args"].as_str() {
                    Some(a) if !a.is_empty() => format!("\n\nUser arguments: {a}"),
                    _ => String::new(),
                };
                let related_footer = render_related_footer(def, &registry);
                ToolResult::ok(format!(
                    "# Skill: {}\n\n{}{args_note}{related_footer}",
                    def.label, def.content
                ))
            }
            None => ToolResult::err(format!(
                "Unknown skill: {skill_name}\n\nAvailable skills:\n{}",
                format_available(registry.list())
            )),
        }
    }
}

/// Render the "Related Skills" follow-up footer appended after a skill
/// body. Resolves each cross-reference against the live registry so:
///
///   * present skills become `/<name>` slash-command suggestions tagged
///     with the resolved label and one-line description, primed for the
///     model to invoke via the same Skill tool;
///   * missing names are listed under a separate header so the model
///     knows the dangling references exist (and can ask the user) but
///     won't try to invoke them.
///
/// Returns an empty string when the skill has no `## Related Skills`
/// section, so existing callers see no behavior change.
fn render_related_footer(skill: &ParsedSkill, registry: &SkillRegistry) -> String {
    if skill.related.is_empty() {
        return String::new();
    }

    let (resolved, unresolved) = registry.resolve_related(skill);
    if resolved.is_empty() && unresolved.is_empty() {
        return String::new();
    }

    let mut out = String::from("\n\n---\n\n## Related skills (chain via the Skill tool)\n\n");
    for r in &resolved {
        out.push_str(&format!("- `/{}` — {}: {}\n", r.name, r.label, r.description));
    }
    if !unresolved.is_empty() {
        out.push_str(
            "\n_Referenced but not currently registered (ask the user to install or skip):_\n",
        );
        for name in &unresolved {
            out.push_str(&format!("- `{name}`\n"));
        }
    }
    out
}

/// Render the skill list for an "unknown skill" error response. Each line:
/// `  - /<name> [provider] — <Label>: <description>`. The provider tag is
/// shown only for non-builtin skills so users can tell where an override
/// is coming from.
fn format_available(skills: &[ParsedSkill]) -> String {
    let mut lines: Vec<String> = skills
        .iter()
        .map(|s| {
            let tag = if s.provider == SkillProvider::Builtin {
                String::new()
            } else {
                format!(" [{}]", s.provider.label())
            };
            format!("  - /{}{tag} — {}: {}", s.name, s.label, s.description)
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    fn ctx_with_workspace(workspace: &'static Path) -> (CancellationToken, ToolContext<'static>) {
        let token = CancellationToken::new();
        let token_box: &'static CancellationToken = Box::leak(Box::new(token.clone()));
        let ctx = ToolContext::new(workspace, false, token_box);
        (token, ctx)
    }

    #[tokio::test]
    async fn invoke_known_builtin_skill() {
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool.execute(json!({"skill": "simplify"}), &ctx).await;
        assert!(!result.is_error, "error: {}", result.content);
        assert!(result.content.contains("Simplify"));
        assert!(result.content.contains("Code Reuse"));
    }

    #[tokio::test]
    async fn invoke_with_args() {
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool
            .execute(
                json!({"skill": "verify", "args": "check the login flow"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Verify"));
        assert!(result.content.contains("check the login flow"));
    }

    #[tokio::test]
    async fn unknown_skill_lists_available_with_provider_tags() {
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool
            .execute(json!({"skill": "nonexistent"}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("Unknown skill"));
        assert!(result.content.contains("/simplify"));
        assert!(result.content.contains("/frontend-dev"));
    }

    #[tokio::test]
    async fn missing_skill_name() {
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("skill"));
    }

    #[tokio::test]
    async fn project_skill_overrides_builtin() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".agents").join("skills").join("simplify");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: simplify\nlabel: Simplify (project)\n\
             description: Project override.\n---\n\nProject body content.",
        )
        .unwrap();

        let workspace: &'static Path = Box::leak(tmp.path().to_path_buf().into_boxed_path());
        let (_token, ctx) = ctx_with_workspace(workspace);
        let tool = SkillTool;
        let result = tool.execute(json!({"skill": "simplify"}), &ctx).await;
        assert!(!result.is_error);
        assert!(
            result.content.contains("Simplify (project)"),
            "got: {}",
            result.content
        );
        assert!(result.content.contains("Project body content"));
    }

    #[tokio::test]
    async fn underscore_alias_resolves_to_kebab_skill() {
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool
            .execute(json!({"skill": "frontend_dev"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Frontend Dev"));
    }

    // ── Related Skills footer ─────────────────────────────────────────────

    #[tokio::test]
    async fn invoke_renders_related_skills_footer() {
        // `simplify` lists `verify` and `implement-specs` as related — both
        // builtins, so both should resolve and appear in the footer.
        let tool = SkillTool;
        let (_token, ctx) = ctx_with_workspace(Path::new("/tmp"));
        let result = tool.execute(json!({"skill": "simplify"}), &ctx).await;
        assert!(!result.is_error);
        assert!(
            result.content.contains("Related skills (chain via the Skill tool)"),
            "footer header missing"
        );
        assert!(
            result.content.contains("`/verify`"),
            "verify not surfaced in footer"
        );
        assert!(
            result.content.contains("`/implement-specs`"),
            "implement-specs not surfaced in footer"
        );
    }

    #[tokio::test]
    async fn footer_is_omitted_when_skill_has_no_related_section() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".agents").join("skills").join("loner");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: loner\ndescription: Stand-alone.\n---\n\n# Body\n\nNo cross-links.",
        )
        .unwrap();

        let workspace: &'static Path = Box::leak(tmp.path().to_path_buf().into_boxed_path());
        let (_token, ctx) = ctx_with_workspace(workspace);
        let tool = SkillTool;
        let result = tool.execute(json!({"skill": "loner"}), &ctx).await;
        assert!(!result.is_error);
        assert!(
            !result.content.contains("Related skills"),
            "footer should not render for skills without a Related Skills section"
        );
    }

    #[tokio::test]
    async fn footer_lists_unresolved_references_separately() {
        // Skill cross-references one real builtin (`verify`) and one
        // dangling name (`does-not-exist`). The footer must surface the
        // resolved one as a `/<name>` suggestion and call out the
        // dangling reference under a distinct header so the model
        // doesn't try to invoke it.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".agents").join("skills").join("mixed");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: mixed\ndescription: Mixed refs.\n---\n\n\
             # Body\n\n## Related Skills\n\n- `verify`\n- `does-not-exist`\n",
        )
        .unwrap();

        let workspace: &'static Path = Box::leak(tmp.path().to_path_buf().into_boxed_path());
        let (_token, ctx) = ctx_with_workspace(workspace);
        let tool = SkillTool;
        let result = tool.execute(json!({"skill": "mixed"}), &ctx).await;
        assert!(!result.is_error);
        assert!(result.content.contains("`/verify`"));
        assert!(result.content.contains("Referenced but not currently registered"));
        assert!(result.content.contains("`does-not-exist`"));
    }

    #[test]
    fn every_p1_builtin_has_related_skills_populated() {
        // Sanity: P4 retrofit must have populated every P1 + P3 builtin
        // with at least one Related Skills entry. Catches a regression
        // where someone removes the section without thinking.
        let reg = crate::bundled_skills::default_skill_registry();
        for s in reg.list() {
            assert!(
                !s.related.is_empty(),
                "builtin `{}` lost its Related Skills section",
                s.name
            );
        }
    }
}
