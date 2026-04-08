mod prompt;

use super::SkillDef;

pub fn skill_def() -> SkillDef {
    SkillDef {
        slug: "ui-design-system",
        label: "UI Design System",
        description: "Design-focused skill for UI polish, visual consistency, and design \
                      system enforcement. Use when existing UI needs to look production-quality.",
        category: "design",
        prompt: prompt::PROMPT,
    }
}
