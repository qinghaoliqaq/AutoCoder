mod prompt;

use super::SkillDef;

pub fn skill_def() -> SkillDef {
    SkillDef {
        slug: "verify",
        label: "Verify",
        description: "End-to-end verification of code changes. Runs the application, \
                      tests functionality, and confirms changes work as intended.",
        category: "testing",
        prompt: prompt::PROMPT,
    }
}
