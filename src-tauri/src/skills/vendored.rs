use super::blackboard::{SubtaskCard, SubtaskKind};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const VENDORED_ROOT: &str = "vendor/minimax-skills";
const MAX_EXCERPT_CHARS: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VendoredSkillId {
    FrontendDev,
    FullstackDev,
}

impl VendoredSkillId {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::FrontendDev => "frontend-dev",
            Self::FullstackDev => "fullstack-dev",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FrontendDev => "MiniMax frontend-dev",
            Self::FullstackDev => "MiniMax fullstack-dev",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VendoredSkill {
    pub id: VendoredSkillId,
    pub root_dir: PathBuf,
    pub skill_path: PathBuf,
    pub excerpt: String,
}

pub(crate) fn select_for_subtask(card: &SubtaskCard) -> Option<VendoredSkillId> {
    if matches!(card.kind, SubtaskKind::Screen) {
        return Some(VendoredSkillId::FrontendDev);
    }

    let text = format!("{} {}", card.title, card.description).to_lowercase();
    let tokens = tokenize(&text);
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
    let ui_phrases = [
        "user interface",
        "front end",
        "front-end",
        "ui layer",
    ];

    let has_ui = ui_keywords.iter().any(|kw| tokens.iter().any(|token| token == kw))
        || ui_phrases.iter().any(|phrase| text.contains(phrase));
    let has_backend = backend_keywords
        .iter()
        .any(|kw| tokens.iter().any(|token| token == kw));

    if has_ui && has_backend {
        Some(VendoredSkillId::FullstackDev)
    } else {
        None
    }
}

pub(crate) fn load(skill_id: VendoredSkillId, app_handle: &AppHandle) -> Result<VendoredSkill, String> {
    let skill_rel = Path::new("skills").join(skill_id.slug());
    let skill_file_rel = skill_rel.join("SKILL.md");

    for root in candidate_roots(app_handle) {
        let skill_path = root.join(&skill_file_rel);
        if skill_path.exists() {
            let content = std::fs::read_to_string(&skill_path)
                .map_err(|e| format!("Cannot read vendored skill {}: {e}", skill_path.display()))?;
            return Ok(VendoredSkill {
                id: skill_id,
                root_dir: root.join(&skill_rel),
                skill_path,
                excerpt: truncate(&content, MAX_EXCERPT_CHARS),
            });
        }
    }

    Err(format!(
        "Vendored skill {} not found in bundled resources or repo vendor directory",
        skill_id.slug()
    ))
}

fn candidate_roots(app_handle: &AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        roots.push(resource_dir.join(VENDORED_ROOT));
        roots.push(resource_dir.join("minimax-skills"));
    }

    if let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        roots.push(repo_root.join(VENDORED_ROOT));
    }

    dedupe_paths(roots)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if !result.iter().any(|existing: &PathBuf| existing == &path) {
            result.push(path);
        }
    }
    result
}

fn truncate(text: &str, max_chars: usize) -> String {
    let truncated: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{truncated}\n\n[truncated]")
    } else {
        truncated
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

    #[test]
    fn select_frontend_skill_for_screen_subtask() {
        let card = SubtaskCard {
            id: "P1".to_string(),
            title: "Dashboard".to_string(),
            description: "Build dashboard screen".to_string(),
            kind: SubtaskKind::Screen,
            status: super::super::blackboard::SubtaskState::Pending,
            attempts: 0,
            latest_implementation: None,
            latest_review: None,
            review_findings: Vec::new(),
            files_touched: Vec::new(),
        };
        assert_eq!(select_for_subtask(&card), Some(VendoredSkillId::FrontendDev));
    }

    #[test]
    fn select_fullstack_skill_for_ui_api_feature() {
        let card = SubtaskCard {
            id: "F1".to_string(),
            title: "Profile api integration".to_string(),
            description: "Wire dashboard form to backend API with auth".to_string(),
            kind: SubtaskKind::Feature,
            status: super::super::blackboard::SubtaskState::Pending,
            attempts: 0,
            latest_implementation: None,
            latest_review: None,
            review_findings: Vec::new(),
            files_touched: Vec::new(),
        };
        assert_eq!(select_for_subtask(&card), Some(VendoredSkillId::FullstackDev));
    }

    #[test]
    fn does_not_treat_build_as_ui_keyword() {
        let card = SubtaskCard {
            id: "F2".to_string(),
            title: "Build auth API endpoint".to_string(),
            description: "Create backend endpoint for login".to_string(),
            kind: SubtaskKind::Feature,
            status: super::super::blackboard::SubtaskState::Pending,
            attempts: 0,
            latest_implementation: None,
            latest_review: None,
            review_findings: Vec::new(),
            files_touched: Vec::new(),
        };
        assert_eq!(select_for_subtask(&card), None);
    }
}
