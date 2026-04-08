/// Director LLM configuration.
///
/// Loading priority (highest first):
///   1. Environment variables
///   2. Persisted config in the app config directory
///   3. Fallback config.toml discovered from cwd / executable parents
///
/// Supported providers:
///   api_format = "openai"     → OpenAI-compatible  (/chat/completions, Bearer token)
///   api_format = "anthropic"  → Anthropic-compatible (/messages, x-api-key header)
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_MAX_PARALLEL_SUBTASKS: usize = 5;
const MAX_PARALLEL_SUBTASKS_CAP: usize = 8;

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
            ApiFormat::OpenAI => "openai",
            ApiFormat::Anthropic => "anthropic",
        }
    }
}

// ── ExecutionAccessMode ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAccessMode {
    #[default]
    Sandbox,
    FullAccess,
}

impl ExecutionAccessMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionAccessMode::Sandbox => "sandbox",
            ExecutionAccessMode::FullAccess => "full_access",
        }
    }
}

// ── Config structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
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
    #[serde(default = "default_max_parallel_subtasks")]
    pub max_parallel_subtasks: usize,
    #[serde(default)]
    pub execution_access_mode: ExecutionAccessMode,
}

/// Agent-layer configuration — used by skills that run via the Anthropic API
/// tool_use loop. If not configured, skills fall back to the legacy CLI runner
/// mode, or use the [director] config if it has api_format = "anthropic".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Anthropic API key (or cloud provider key).
    #[serde(default)]
    pub api_key:  String,
    /// Custom base URL for API proxy / self-hosted endpoint.
    /// Leave empty to use Anthropic's default endpoint.
    #[serde(default)]
    pub base_url: String,
    /// Model to use for skill execution (e.g. "claude-sonnet-4-6").
    #[serde(default = "default_agent_model")]
    pub model:    String,
    /// Provider: "anthropic" (default), "bedrock", "vertex", "foundry".
    #[serde(default = "default_provider")]
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub director: DirectorConfig,
    #[serde(default)]
    pub features: FeaturesConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

/// Returned to the frontend — API key is masked for security.
#[derive(Debug, Serialize)]
pub struct ConfigStatus {
    pub configured: bool,
    pub base_url: String,
    pub model: String,
    pub api_format: String,
    pub api_key_hint: String,
    pub vendored_skills: bool,
    pub max_parallel_subtasks: usize,
    pub execution_access_mode: String,
}

/// Editable config payload used by the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDraft {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub api_format: ApiFormat,
    pub vendored_skills: bool,
    pub max_parallel_subtasks: usize,
    pub execution_access_mode: ExecutionAccessMode,
    // Agent layer
    #[serde(default)]
    pub agent_provider: String,
    #[serde(default)]
    pub agent_api_key: String,
    #[serde(default)]
    pub agent_base_url: String,
    #[serde(default)]
    pub agent_model: String,
}

// ── Defaults ──────────────────────────────────────────────────────────────────

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            api_format: ApiFormat::OpenAI,
            context_budget: default_context_budget(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            api_key:  String::new(),
            base_url: String::new(),
            model:    default_agent_model(),
            provider: default_provider(),
        }
    }
}

impl AgentConfig {
    /// Returns true if enough configuration is present to launch the Agent SDK sidecar.
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            director: DirectorConfig::default(),
            features: FeaturesConfig::default(),
            agent:    AgentConfig::default(),
        }
    }
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            vendored_skills: default_true(),
            max_parallel_subtasks: default_max_parallel_subtasks(),
            execution_access_mode: ExecutionAccessMode::default(),
        }
    }
}

impl FeaturesConfig {
    pub fn parallel_subtask_limit(&self) -> usize {
        clamp_parallel_subtasks(self.max_parallel_subtasks)
    }
}

// ── Loading ───────────────────────────────────────────────────────────────────

impl AppConfig {
    pub fn load() -> Self {
        let mut cfg = Self::load_persisted().unwrap_or_default();
        Self::apply_env_overrides(&mut cfg);
        cfg
    }

    pub fn load_persisted() -> Option<Self> {
        for path in config_search_paths_for_load() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Self>(&content) {
                    return Some(cfg);
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
                _ => ApiFormat::OpenAI,
            };
        }

        // Context budget
        if let Ok(v) = std::env::var("DIRECTOR_CONTEXT_BUDGET") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.director.context_budget = n;
            }
        }

        // Agent layer
        if let Ok(v) = std::env::var("AGENT_API_KEY") {
            cfg.agent.api_key = v;
        }
        if let Ok(v) = std::env::var("AGENT_BASE_URL") {
            cfg.agent.base_url = v;
        }
        if let Ok(v) = std::env::var("AGENT_MODEL") {
            cfg.agent.model = v;
        }
        if let Ok(v) = std::env::var("AGENT_PROVIDER") {
            cfg.agent.provider = v;
        }

        if let Ok(v) = std::env::var("AI_DEV_HUB_VENDORED_SKILLS") {
            cfg.features.vendored_skills = parse_bool(&v).unwrap_or(cfg.features.vendored_skills);
        }
        if let Ok(v) = std::env::var("AI_DEV_HUB_MAX_PARALLEL_SUBTASKS") {
            cfg.features.max_parallel_subtasks = parse_usize(&v)
                .map(clamp_parallel_subtasks)
                .unwrap_or_else(|| cfg.features.parallel_subtask_limit());
        }
        if let Ok(v) = std::env::var("AI_DEV_HUB_EXECUTION_ACCESS_MODE") {
            cfg.features.execution_access_mode =
                parse_execution_access_mode(&v).unwrap_or(cfg.features.execution_access_mode);
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.director.api_key.trim().is_empty()
            && !self.director.base_url.trim().is_empty()
            && !self.director.model.trim().is_empty()
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
            configured: self.is_configured(),
            base_url: self.director.base_url.clone(),
            model: self.director.model.clone(),
            api_format: self.director.api_format.as_str().to_string(),
            api_key_hint: hint,
            vendored_skills: self.features.vendored_skills,
            max_parallel_subtasks: self.features.parallel_subtask_limit(),
            execution_access_mode: self.features.execution_access_mode.as_str().to_string(),
        }
    }

    pub fn draft(&self) -> ConfigDraft {
        ConfigDraft {
            api_key: self.director.api_key.clone(),
            base_url: self.director.base_url.clone(),
            model: self.director.model.clone(),
            api_format: self.director.api_format.clone(),
            vendored_skills: self.features.vendored_skills,
            max_parallel_subtasks: self.features.parallel_subtask_limit(),
            execution_access_mode: self.features.execution_access_mode,
            agent_provider: self.agent.provider.clone(),
            agent_api_key: self.agent.api_key.clone(),
            agent_base_url: self.agent.base_url.clone(),
            agent_model: self.agent.model.clone(),
        }
    }

    pub fn persist_draft(draft: ConfigDraft) -> Result<Self, String> {
        let api_key = draft.api_key.trim().to_string();
        let base_url = draft.base_url.trim().to_string();
        let model = draft.model.trim().to_string();

        if base_url.is_empty() {
            return Err("Base URL cannot be empty".to_string());
        }
        if model.is_empty() {
            return Err("Model cannot be empty".to_string());
        }

        let cfg = AppConfig {
            director: DirectorConfig {
                api_key,
                base_url,
                model,
                api_format: draft.api_format,
                context_budget: default_context_budget(),
            },
            features: FeaturesConfig {
                vendored_skills: draft.vendored_skills,
                max_parallel_subtasks: clamp_parallel_subtasks(draft.max_parallel_subtasks),
                execution_access_mode: draft.execution_access_mode,
            },
            agent: AgentConfig {
                api_key: draft.agent_api_key.trim().to_string(),
                base_url: draft.agent_base_url.trim().to_string(),
                model: if draft.agent_model.trim().is_empty() {
                    default_agent_model()
                } else {
                    draft.agent_model.trim().to_string()
                },
                provider: if draft.agent_provider.trim().is_empty() {
                    default_provider()
                } else {
                    draft.agent_provider.trim().to_lowercase()
                },
            },
        };

        let path = writable_config_path()?;
        let content = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("Cannot serialize config.toml: {e}"))?;
        std::fs::write(&path, format!("{content}\n"))
            .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
        Ok(AppConfig::load())
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

fn writable_config_path() -> Result<PathBuf, String> {
    if let Some(dir) = app_config_dir() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create config directory {}: {e}", dir.display()))?;
        return Ok(dir.join("config.toml"));
    }

    Err("Unable to determine where config.toml should be written".to_string())
}

fn config_search_paths_for_load() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(dir) = app_config_dir() {
        let path = dir.join("config.toml");
        if path.exists() {
            paths.push(path);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(path) = walk_up_for_config(&cwd) {
            paths.push(path);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(path) = walk_up_for_config(dir) {
                if !paths.iter().any(|existing| existing == &path) {
                    paths.push(path);
                }
            }
        }
    }

    paths
}

fn app_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("ai-dev-hub"))
}

fn default_true() -> bool {
    true
}

fn default_context_budget() -> usize {
    24_000
}

fn default_agent_model() -> String {
    "claude-sonnet-4-6".to_string()
}

fn default_provider() -> String {
    "anthropic".to_string()
}

fn default_max_parallel_subtasks() -> usize {
    DEFAULT_MAX_PARALLEL_SUBTASKS
}

fn clamp_parallel_subtasks(value: usize) -> usize {
    value.clamp(1, MAX_PARALLEL_SUBTASKS_CAP)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_usize(value: &str) -> Option<usize> {
    value.trim().parse::<usize>().ok()
}

fn parse_execution_access_mode(value: &str) -> Option<ExecutionAccessMode> {
    match value.trim().to_lowercase().as_str() {
        "sandbox" => Some(ExecutionAccessMode::Sandbox),
        "full_access" | "full-access" | "full" => Some(ExecutionAccessMode::FullAccess),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_default_to_sandbox_execution_mode() {
        let features = FeaturesConfig::default();
        assert_eq!(features.execution_access_mode, ExecutionAccessMode::Sandbox);
    }

    #[test]
    fn config_status_exposes_execution_access_mode() {
        let mut config = AppConfig::default();
        config.features.execution_access_mode = ExecutionAccessMode::FullAccess;
        assert_eq!(config.status().execution_access_mode, "full_access");
    }

    #[test]
    fn parse_execution_access_mode_accepts_aliases() {
        assert_eq!(
            parse_execution_access_mode("sandbox"),
            Some(ExecutionAccessMode::Sandbox)
        );
        assert_eq!(
            parse_execution_access_mode("full-access"),
            Some(ExecutionAccessMode::FullAccess)
        );
        assert_eq!(
            parse_execution_access_mode("full_access"),
            Some(ExecutionAccessMode::FullAccess)
        );
        assert_eq!(parse_execution_access_mode("nope"), None);
    }
}
