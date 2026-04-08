mod prompt;

use super::SkillDef;

pub fn skill_def() -> SkillDef {
    SkillDef {
        slug: "fullstack-dev",
        label: "Fullstack Dev",
        description: "Lightweight full-stack implementation guide. Use for subtasks that \
                      cross frontend and backend boundaries: API-backed features, auth flows, \
                      CRUD modules, data synchronization, validation.",
        category: "full-stack",
        prompt: prompt::PROMPT,
    }
}
