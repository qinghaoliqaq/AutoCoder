/// Bundled skills registry — prompt-based contextual guides for the coding agent.
///
/// Skills are NOT executable tools. They are domain-specific instruction sets
/// that get injected into the system prompt (or returned by the Skill tool)
/// to guide the model's behavior for specific task types.
///
/// ```text
/// bundled_skills/
///   mod.rs              ← SkillDef, SkillRegistry (this file)
///   frontend_dev/       ← Frontend implementation guide
///   fullstack_dev/      ← Full-stack feature guide
///   ui_design_system/   ← UI polish and visual consistency
///   simplify/           ← Code review: reuse, quality, efficiency
///   verify/             ← End-to-end verification of changes
/// ```
pub mod frontend_dev;
pub mod fullstack_dev;
pub mod simplify;
pub mod ui_design_system;
pub mod verify;

use std::collections::HashMap;

/// A bundled skill definition.
pub struct SkillDef {
    /// Machine name / slug (e.g. "frontend-dev").
    pub slug: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// Short description (shown in skill listings).
    pub description: &'static str,
    /// Category for grouping (e.g. "frontend", "review", "testing").
    pub category: &'static str,
    /// Full prompt content — the entire instruction set injected into context.
    pub prompt: &'static str,
}

/// Registry holding all bundled skills.
pub struct SkillRegistry {
    skills: Vec<SkillDef>,
    by_slug: HashMap<&'static str, usize>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            by_slug: HashMap::new(),
        }
    }

    /// Register a skill. Panics on duplicate slug.
    pub fn register(&mut self, skill: SkillDef) {
        assert!(
            !self.by_slug.contains_key(skill.slug),
            "duplicate skill: {}",
            skill.slug
        );
        let idx = self.skills.len();
        self.by_slug.insert(skill.slug, idx);
        self.skills.push(skill);
    }

    /// Number of registered skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Look up a skill by slug.
    pub fn get(&self, slug: &str) -> Option<&SkillDef> {
        self.by_slug.get(slug).map(|&idx| &self.skills[idx])
    }

    /// List all registered skills (slug, label, description, category).
    pub fn list(&self) -> Vec<(&str, &str, &str, &str)> {
        self.skills
            .iter()
            .map(|s| (s.slug, s.label, s.description, s.category))
            .collect()
    }

}

/// Build the default skill registry with all bundled skills.
pub fn default_skill_registry() -> SkillRegistry {
    let mut reg = SkillRegistry::new();

    reg.register(frontend_dev::skill_def());
    reg.register(fullstack_dev::skill_def());
    reg.register(ui_design_system::skill_def());
    reg.register(simplify::skill_def());
    reg.register(verify::skill_def());

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_skills() {
        let reg = default_skill_registry();
        assert_eq!(reg.len(), 5);
    }

    #[test]
    fn lookup_by_slug() {
        let reg = default_skill_registry();
        let skill = reg.get("frontend-dev").unwrap();
        assert_eq!(skill.label, "Frontend Dev");
        assert!(!skill.prompt.is_empty());
    }

    #[test]
    fn list_returns_all() {
        let reg = default_skill_registry();
        let list = reg.list();
        assert_eq!(list.len(), 5);
        let slugs: Vec<&str> = list.iter().map(|s| s.0).collect();
        assert!(slugs.contains(&"frontend-dev"));
        assert!(slugs.contains(&"fullstack-dev"));
        assert!(slugs.contains(&"ui-design-system"));
        assert!(slugs.contains(&"simplify"));
        assert!(slugs.contains(&"verify"));
    }
}
