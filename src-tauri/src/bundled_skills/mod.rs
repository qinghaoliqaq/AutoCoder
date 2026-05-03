//! Bundled skills — Warp/Claude/Codex-compatible markdown skill registry.
//!
//! Skills are markdown documents with YAML frontmatter (`name`, `description`,
//! optional `label` / `category`). The Skill tool exposes them to the model as
//! "slash commands"; the model picks one based on the description and follows
//! the skill body's instructions.
//!
//! ## Layout
//! ```text
//! bundled_skills/
//!   mod.rs                              ← public API (this file)
//!   parsed_skill.rs                     ← ParsedSkill type + frontmatter parser
//!   loader.rs                           ← discovery across builtin + project + user dirs
//!   simplify/SKILL.md                   ← compiled-in builtins
//!   verify/SKILL.md
//!   frontend_dev/SKILL.md
//!   fullstack_dev/SKILL.md
//!   ui_design_system/SKILL.md
//!   write_tech_spec/SKILL.md            ← spec-driven workflow
//!   implement_specs/SKILL.md
//!   spec_driven_implementation/SKILL.md
//! ```
//!
//! ## Discovery sources (highest priority first)
//! 1. `<workspace>/.agents/skills/<name>/SKILL.md`
//! 2. `~/.config/ai-dev-hub/skills/<name>/SKILL.md`
//! 3. `~/.claude/skills/<name>/SKILL.md`           — interop with Claude Code
//! 4. `~/.codex/skills/<name>/SKILL.md`            — interop with Codex
//! 5. Compiled-in builtins
//!
//! Duplicate names resolve to the highest-priority provider (see
//! `SkillProvider::rank`).

pub mod loader;
pub mod parsed_skill;

use std::path::Path;

pub use loader::{dedupe_by_priority, discover_all, load_builtins};
pub use parsed_skill::{ParsedSkill, SkillProvider};
// `SkillScope` and `SkillSource` are reachable via `parsed_skill::*` for
// callers that need them; we don't re-export from the crate root because
// today no external code consumes them and the unused-import lint would
// trip CI's `-D warnings`.

/// Read-only view over a deduplicated set of skills, indexed by name.
pub struct SkillRegistry {
    skills: Vec<ParsedSkill>,
}

impl SkillRegistry {
    /// Build a registry from builtins plus all on-disk skill sources rooted
    /// at `workspace` (project skills) and the user's home directories
    /// (user / Claude / Codex skills).
    pub fn discover(workspace: Option<&Path>) -> Self {
        Self {
            skills: dedupe_by_priority(discover_all(workspace)),
        }
    }

    /// Builtin-only registry. Use when a workspace path isn't available.
    pub fn builtins_only() -> Self {
        Self {
            skills: dedupe_by_priority(load_builtins()),
        }
    }

    /// Look up by exact kebab-case name.
    pub fn get(&self, name: &str) -> Option<&ParsedSkill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// Resolve a user-typed skill identifier, accepting `_` ↔ `-` swaps and
    /// case variants. Used by the Skill tool to be forgiving about how
    /// the model writes the slug.
    pub fn resolve(&self, query: &str) -> Option<&ParsedSkill> {
        if let Some(s) = self.get(query) {
            return Some(s);
        }
        let normalized = query.trim().to_ascii_lowercase().replace('_', "-");
        self.get(&normalized)
    }

    pub fn list(&self) -> &[ParsedSkill] {
        &self.skills
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Resolve a skill's `Related Skills` cross-references against this
    /// registry. Returns `(resolved, unresolved)` — a parallel pair of the
    /// found `ParsedSkill`s and the names that didn't match anything. Used
    /// by the `Skill` tool to render a discoverable next-step footer.
    pub fn resolve_related<'a>(
        &'a self,
        skill: &ParsedSkill,
    ) -> (Vec<&'a ParsedSkill>, Vec<String>) {
        let mut resolved = Vec::new();
        let mut unresolved = Vec::new();
        for name in &skill.related {
            match self.get(name) {
                Some(s) => resolved.push(s),
                None => unresolved.push(name.clone()),
            }
        }
        (resolved, unresolved)
    }
}

/// Default registry: builtins only. Prefer `SkillRegistry::discover(workspace)`
/// from any code path that has a workspace path available — that picks up
/// project skills and Claude/Codex skill directories too.
pub fn default_skill_registry() -> SkillRegistry {
    SkillRegistry::builtins_only()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_only_has_all_eight_skills() {
        let reg = default_skill_registry();
        assert_eq!(reg.len(), 8);
    }

    #[test]
    fn lookup_each_builtin_by_name() {
        let reg = default_skill_registry();
        for n in [
            "simplify",
            "verify",
            "frontend-dev",
            "fullstack-dev",
            "ui-design-system",
            "write-tech-spec",
            "implement-specs",
            "spec-driven-implementation",
        ] {
            let s = reg.get(n).unwrap_or_else(|| panic!("missing {n}"));
            assert_eq!(s.name, n);
            assert!(!s.content.is_empty());
            assert!(!s.description.is_empty());
        }
    }

    #[test]
    fn resolve_normalizes_underscores_and_case() {
        let reg = default_skill_registry();
        assert!(reg.resolve("frontend_dev").is_some());
        assert!(reg.resolve("Frontend-Dev").is_some());
        assert!(reg.resolve("  ui_design_system  ").is_some());
        assert!(reg.resolve("nonexistent").is_none());
    }

    #[test]
    fn list_returns_all_builtins_with_metadata() {
        let reg = default_skill_registry();
        let list = reg.list();
        assert_eq!(list.len(), 8);
        // Every builtin has the Builtin provider.
        for s in list {
            assert_eq!(s.provider, SkillProvider::Builtin);
        }
    }
}
