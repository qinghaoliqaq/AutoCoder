mod prompt;

use super::SkillDef;

pub fn skill_def() -> SkillDef {
    SkillDef {
        slug: "frontend-dev",
        label: "Frontend Dev",
        description: "Packaged frontend implementation guide. Use for UI-heavy subtasks: \
                      pages, dashboards, forms, landing sections, component composition, \
                      responsive layout, interaction polish.",
        category: "frontend",
        prompt: prompt::PROMPT,
    }
}
