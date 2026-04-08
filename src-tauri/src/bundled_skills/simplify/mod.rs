mod prompt;

use super::SkillDef;

pub fn skill_def() -> SkillDef {
    SkillDef {
        slug: "simplify",
        label: "Simplify",
        description: "Code review for reuse, quality, and efficiency. Launches 3 parallel \
                      review passes: code reuse, code quality, and efficiency optimization.",
        category: "review",
        prompt: prompt::PROMPT,
    }
}
