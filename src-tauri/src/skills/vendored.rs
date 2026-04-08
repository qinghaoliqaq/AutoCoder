use super::blackboard::{SubtaskCard, SubtaskKind};
use crate::bundled_skills;
use super::planning_schema::SuggestedSkill;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VendoredSkillId {
    FrontendDev,
    FullstackDev,
    UiDesignSystem,
}

impl VendoredSkillId {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::FrontendDev => "frontend-dev",
            Self::FullstackDev => "fullstack-dev",
            Self::UiDesignSystem => "ui-design-system",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FrontendDev => "Frontend Dev",
            Self::FullstackDev => "Fullstack Dev",
            Self::UiDesignSystem => "UI Design System",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VendoredSkill {
    pub id: VendoredSkillId,
    pub excerpt: String,
}

pub(crate) fn select_for_subtask(card: &SubtaskCard) -> Option<VendoredSkillId> {
    if let Some(skill) = &card.suggested_skill {
        return Some(match skill {
            SuggestedSkill::FrontendDev => VendoredSkillId::FrontendDev,
            SuggestedSkill::FullstackDev => VendoredSkillId::FullstackDev,
            SuggestedSkill::UiDesignSystem => VendoredSkillId::UiDesignSystem,
        });
    }

    if matches!(card.kind, SubtaskKind::Screen) {
        return Some(VendoredSkillId::FrontendDev);
    }

    let text = format!("{} {}", card.title, card.description).to_lowercase();
    let tokens = tokenize(&text);

    // Check for design/polish subtasks first
    let design_keywords = [
        "beautify",
        "polish",
        "redesign",
        "visual",
        "spacing",
        "typography",
    ];
    let design_phrases = [
        "design system",
        "look and feel",
        "ui polish",
        "visual consistency",
        "color palette",
        "micro-interaction",
    ];
    let has_design = design_keywords
        .iter()
        .any(|kw| tokens.iter().any(|token| token == kw))
        || design_phrases.iter().any(|phrase| text.contains(phrase));
    if has_design {
        return Some(VendoredSkillId::UiDesignSystem);
    }

    let ui_keywords = [
        "screen",
        "page",
        "view",
        "frontend",
        "dashboard",
        "form",
        "modal",
        "upload",
        "table",
    ];
    let backend_keywords = [
        "api",
        "backend",
        "server",
        "endpoint",
        "database",
        "auth",
        "http",
        "rest",
        "crud",
        "integration",
    ];
    let ui_phrases = ["user interface", "front end", "front-end", "ui layer"];

    let has_ui = ui_keywords
        .iter()
        .any(|kw| tokens.iter().any(|token| token == kw))
        || ui_phrases.iter().any(|phrase| text.contains(phrase));
    let has_backend = backend_keywords
        .iter()
        .any(|kw| tokens.iter().any(|token| token == kw));

    if has_ui && has_backend {
        Some(VendoredSkillId::FullstackDev)
    } else if has_ui {
        Some(VendoredSkillId::FrontendDev)
    } else {
        None
    }
}

/// Load a vendored skill from the bundled skills registry.
/// No longer reads from the filesystem — skills are compiled into the binary.
pub(crate) fn load(skill_id: VendoredSkillId) -> Result<VendoredSkill, String> {
    let registry = bundled_skills::default_skill_registry();
    match registry.get(skill_id.slug()) {
        Some(def) => Ok(VendoredSkill {
            id: skill_id,
            excerpt: def.prompt.to_string(),
        }),
        None => Err(format!(
            "Bundled skill {} not found in registry",
            skill_id.slug()
        )),
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(title: &str, desc: &str, kind: SubtaskKind) -> SubtaskCard {
        SubtaskCard {
            id: "T1".to_string(),
            title: title.to_string(),
            description: desc.to_string(),
            kind,
            depends_on: Vec::new(),
            can_run_in_parallel: true,
            parallel_group: None,
            suggested_skill: None,
            expected_touch: Vec::new(),
            status: super::super::blackboard::SubtaskState::Pending,
            attempts: 0,
            latest_implementation: None,
            latest_review: None,
            review_findings: Vec::new(),
            files_touched: Vec::new(),
            isolated_workspace: None,
            merge_conflict: None,
            attempted_fixes: Vec::new(),
        }
    }

    #[test]
    fn select_frontend_skill_for_screen_subtask() {
        let card = make_card("Dashboard", "Build dashboard screen", SubtaskKind::Screen);
        assert_eq!(
            select_for_subtask(&card),
            Some(VendoredSkillId::FrontendDev)
        );
    }

    #[test]
    fn select_fullstack_skill_for_ui_api_feature() {
        let card = make_card(
            "Profile api integration",
            "Wire dashboard form to backend API with auth",
            SubtaskKind::Feature,
        );
        assert_eq!(
            select_for_subtask(&card),
            Some(VendoredSkillId::FullstackDev)
        );
    }

    #[test]
    fn does_not_treat_build_as_ui_keyword() {
        let card = make_card(
            "Build auth API endpoint",
            "Create backend endpoint for login",
            SubtaskKind::Feature,
        );
        assert_eq!(select_for_subtask(&card), None);
    }

    #[test]
    fn select_design_skill_for_polish_subtask() {
        let card = make_card(
            "Polish dashboard",
            "Beautify the main dashboard with visual consistency",
            SubtaskKind::Feature,
        );
        assert_eq!(
            select_for_subtask(&card),
            Some(VendoredSkillId::UiDesignSystem)
        );
    }

    #[test]
    fn select_design_skill_via_suggested_skill() {
        let mut card = make_card("Fix layout", "Fix spacing issues", SubtaskKind::Feature);
        card.suggested_skill = Some(SuggestedSkill::UiDesignSystem);
        assert_eq!(
            select_for_subtask(&card),
            Some(VendoredSkillId::UiDesignSystem)
        );
    }
}
