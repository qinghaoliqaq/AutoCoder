/// Director LLM configuration.
///
/// Loading priority (highest first):
///   1. Environment variables
///   2. config.toml (searched by walking up from the binary and from cwd)
///
/// Supported providers:
///   api_format = "openai"     → OpenAI-compatible  (/chat/completions, Bearer token)
///   api_format = "anthropic"  → Anthropic-compatible (/messages, x-api-key header)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── ApiFormat ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    #[default]
    OpenAI,
    Anthropic,
}

impl ApiFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiFormat::OpenAI     => "openai",
            ApiFormat::Anthropic  => "anthropic",
        }
    }
}

// ── Config structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorConfig {
    pub api_key:    String,
    pub base_url:   String,
    pub model:      String,
    #[serde(default)]
    pub api_format: ApiFormat,
    /// Approximate context budget in tokens for conversation history.
    /// When estimated history tokens exceed this, older messages are compacted
    /// into a structured summary. Defaults to 24 000 tokens.
    #[serde(default = "default_context_budget")]
    pub context_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesConfig {
    #[serde(default = "default_true")]
    pub vendored_skills: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub director: DirectorConfig,
    #[serde(default)]
    pub features: FeaturesConfig,
}

/// Returned to the frontend — API key is masked for security.
#[derive(Debug, Serialize)]
pub struct ConfigStatus {
    pub configured:   bool,
    pub base_url:     String,
    pub model:        String,
    pub api_format:   String,
    pub api_key_hint: String,
    pub vendored_skills: bool,
}

// ── Defaults ──────────────────────────────────────────────────────────────────

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            api_key:    String::new(),
            base_url:   "https://api.openai.com/v1".to_string(),
            model:      "gpt-4o".to_string(),
            api_format: ApiFormat::OpenAI,
            context_budget: default_context_budget(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            director: DirectorConfig::default(),
            features: FeaturesConfig::default(),
        }
    }
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            vendored_skills: default_true(),
        }
    }
}

// ── Loading ───────────────────────────────────────────────────────────────────

impl AppConfig {
    pub fn load() -> Self {
        let mut cfg = Self::load_from_file().unwrap_or_default();
        Self::apply_env_overrides(&mut cfg);
        cfg
    }

    fn load_from_file() -> Option<Self> {
        let mut roots: Vec<PathBuf> = Vec::new();

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.to_path_buf());
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd);
        }

        for root in roots {
            if let Some(path) = walk_up_for_config(&root) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(cfg) = toml::from_str::<Self>(&content) {
                        return Some(cfg);
                    }
                }
            }
        }
        None
    }

    fn apply_env_overrides(cfg: &mut Self) {
        // API key
        for var in &["DIRECTOR_API_KEY", "AGENT_MINIMAX_API_KEY"] {
            if let Ok(v) = std::env::var(var) {
                cfg.director.api_key = v;
                break;
            }
        }
        // Base URL
        for var in &["DIRECTOR_BASE_URL", "AGENT_MINIMAX_BASE_URL"] {
            if let Ok(v) = std::env::var(var) {
                cfg.director.base_url = v;
                break;
            }
        }
        // Model
        for var in &["DIRECTOR_MODEL", "AGENT_MINIMAX_MODEL"] {
            if let Ok(v) = std::env::var(var) {
                cfg.director.model = v;
                break;
            }
        }
        // Format
        if let Ok(v) = std::env::var("DIRECTOR_API_FORMAT") {
            cfg.director.api_format = match v.to_lowercase().as_str() {
                "anthropic" => ApiFormat::Anthropic,
                _           => ApiFormat::OpenAI,
            };
        }

        // Context budget
        if let Ok(v) = std::env::var("DIRECTOR_CONTEXT_BUDGET") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.director.context_budget = n;
            }
        }

        if let Ok(v) = std::env::var("AI_DEV_HUB_VENDORED_SKILLS") {
            cfg.features.vendored_skills = parse_bool(&v).unwrap_or(cfg.features.vendored_skills);
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.director.api_key.is_empty() && !self.director.base_url.is_empty()
    }

    pub fn status(&self) -> ConfigStatus {
        let key = &self.director.api_key;
        let hint = if key.len() >= 8 {
            format!("{}****{}", &key[..4], &key[key.len() - 4..])
        } else if key.is_empty() {
            "(not set)".to_string()
        } else {
            "****".to_string()
        };

        ConfigStatus {
            configured:   self.is_configured(),
            base_url:     self.director.base_url.clone(),
            model:        self.director.model.clone(),
            api_format:   self.director.api_format.as_str().to_string(),
            api_key_hint: hint,
            vendored_skills: self.features.vendored_skills,
        }
    }
}

// ── File helpers ──────────────────────────────────────────────────────────────

fn walk_up_for_config(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..10 {
        let candidate = dir.join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn default_true() -> bool {
    true
}

fn default_context_budget() -> usize {
    24_000
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
