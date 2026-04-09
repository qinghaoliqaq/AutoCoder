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
    /// When enabled, code mode runs compile/type-check commands after
    /// implementation and before review.  Auto-detects build system.
    #[serde(default = "default_true")]
    pub build_gate: bool,
}

/// Agent-layer configuration — used by skills that run via the API tool_use loop.
/// If not configured, falls back to the [director] config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Anthropic API key (or cloud provider key).
    #[serde(default)]
    pub api_key: String,
    /// Custom base URL for API proxy / self-hosted endpoint.
    /// Leave empty to use Anthropic's default endpoint.
    #[serde(default)]
    pub base_url: String,
    /// Model to use for skill execution (e.g. "claude-sonnet-4-0").
    #[serde(default = "default_agent_model")]
    pub model: String,
    /// Provider: "anthropic" (default), "bedrock", "vertex", "foundry".
    #[serde(default = "default_provider")]
    pub provider: String,
    // ── Second identity (Codex) ──────────────────────────────────────
    /// Provider for the second identity.  Falls back to primary `provider`.
    #[serde(default)]
    pub second_provider: String,
    /// API key for the second identity.  Falls back to primary `api_key`.
    #[serde(default)]
    pub second_api_key: String,
    /// Base URL for the second identity.  Falls back to primary `base_url`.
    #[serde(default)]
    pub second_base_url: String,
    /// Model for the second identity.  Falls back to primary `model`.
    #[serde(default)]
    pub second_model: String,
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
    /// Agent primary provider identifier (e.g. "anthropic", "openai", "deepseek").
    pub agent_provider: String,
    /// Agent secondary provider identifier. Empty means follows primary.
    pub agent_second_provider: String,
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
    #[serde(default)]
    pub agent_second_provider: String,
    #[serde(default)]
    pub agent_second_api_key: String,
    #[serde(default)]
    pub agent_second_base_url: String,
    #[serde(default)]
    pub agent_second_model: String,
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
            api_key: String::new(),
            base_url: String::new(),
            model: default_agent_model(),
            provider: default_provider(),
            second_provider: String::new(),
            second_api_key: String::new(),
            second_base_url: String::new(),
            second_model: String::new(),
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
            agent: AgentConfig::default(),
        }
    }
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            vendored_skills: default_true(),
            max_parallel_subtasks: default_max_parallel_subtasks(),
            execution_access_mode: ExecutionAccessMode::default(),
            build_gate: default_true(),
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
        normalize_agent_config(&mut cfg.agent);
        cfg
    }

    pub fn load_persisted() -> Option<Self> {
        for path in config_search_paths_for_load() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                match toml::from_str::<Self>(&content) {
                    Ok(cfg) => return Some(cfg),
                    Err(e) => {
                        tracing::warn!("Config file {} has invalid TOML, skipping: {e}", path.display());
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
        // Second identity overrides
        if let Ok(v) = std::env::var("AGENT_SECOND_PROVIDER") {
            cfg.agent.second_provider = v;
        }
        if let Ok(v) = std::env::var("AGENT_SECOND_API_KEY") {
            cfg.agent.second_api_key = v;
        }
        if let Ok(v) = std::env::var("AGENT_SECOND_BASE_URL") {
            cfg.agent.second_base_url = v;
        }
        if let Ok(v) = std::env::var("AGENT_SECOND_MODEL") {
            cfg.agent.second_model = v;
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
        let key_chars: Vec<char> = key.chars().collect();
        let hint = if key_chars.len() >= 8 {
            let prefix: String = key_chars[..4].iter().collect();
            let suffix: String = key_chars[key_chars.len() - 4..].iter().collect();
            format!("{prefix}****{suffix}")
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
            agent_provider: self.agent.provider.clone(),
            agent_second_provider: self.agent.second_provider.clone(),
        }
    }

    pub fn draft(&self) -> ConfigDraft {
        let mut agent = self.agent.clone();
        normalize_agent_config(&mut agent);
        ConfigDraft {
            api_key: self.director.api_key.clone(),
            base_url: self.director.base_url.clone(),
            model: self.director.model.clone(),
            api_format: self.director.api_format.clone(),
            vendored_skills: self.features.vendored_skills,
            max_parallel_subtasks: self.features.parallel_subtask_limit(),
            execution_access_mode: self.features.execution_access_mode,
            agent_provider: agent.provider.clone(),
            agent_api_key: agent.api_key.clone(),
            agent_base_url: agent.base_url.clone(),
            agent_model: agent.model.clone(),
            agent_second_provider: agent.second_provider.clone(),
            agent_second_api_key: agent.second_api_key.clone(),
            agent_second_base_url: agent.second_base_url.clone(),
            agent_second_model: agent.second_model.clone(),
        }
    }

    pub fn persist_draft(draft: ConfigDraft) -> Result<Self, String> {
        let api_key = draft.api_key.trim().to_string();
        let base_url = draft.base_url.trim().to_string();
        let model = draft.model.trim().to_string();
        let existing = AppConfig::load_persisted().unwrap_or_default();

        if base_url.is_empty() {
            return Err("Base URL cannot be empty".to_string());
        }
        if model.is_empty() {
            return Err("Model cannot be empty".to_string());
        }

        let mut cfg = AppConfig {
            director: DirectorConfig {
                api_key,
                base_url,
                model,
                api_format: draft.api_format,
                context_budget: existing.director.context_budget,
            },
            features: FeaturesConfig {
                vendored_skills: draft.vendored_skills,
                max_parallel_subtasks: clamp_parallel_subtasks(draft.max_parallel_subtasks),
                execution_access_mode: draft.execution_access_mode,
                build_gate: existing.features.build_gate,
            },
            agent: AgentConfig {
                api_key: draft.agent_api_key.trim().to_string(),
                base_url: draft.agent_base_url.trim().to_string(),
                model: draft.agent_model.trim().to_string(),
                provider: draft.agent_provider.trim().to_lowercase(),
                second_provider: draft.agent_second_provider.trim().to_lowercase(),
                second_api_key: draft.agent_second_api_key.trim().to_string(),
                second_base_url: draft.agent_second_base_url.trim().to_string(),
                second_model: draft.agent_second_model.trim().to_string(),
            },
        };
        normalize_agent_config(&mut cfg.agent);

        let path = writable_config_path()?;
        let content = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("Cannot serialize config.toml: {e}"))?;
        // Atomic write: tmp + rename to prevent corrupt config on crash.
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, format!("{content}\n"))
            .map_err(|e| format!("Cannot write {}: {e}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Cannot rename config {}: {e}", path.display()))?;
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
    "claude-sonnet-4-0".to_string()
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

fn normalize_agent_config(agent: &mut AgentConfig) {
    normalize_agent_identity(&mut agent.provider, &mut agent.base_url, &mut agent.model, None);
    let fallback_provider = if agent.second_provider.trim().is_empty() {
        Some(agent.provider.as_str())
    } else {
        None
    };
    normalize_agent_identity(
        &mut agent.second_provider,
        &mut agent.second_base_url,
        &mut agent.second_model,
        fallback_provider,
    );
}

fn normalize_agent_identity(
    provider: &mut String,
    base_url: &mut String,
    model: &mut String,
    fallback_provider: Option<&str>,
) {
    let effective_provider = if provider.trim().is_empty() {
        fallback_provider.unwrap_or("")
    } else {
        provider.as_str()
    }
    .trim()
    .to_lowercase();

    if effective_provider.is_empty() || effective_provider == "anthropic" {
        return;
    }

    if is_legacy_claude_default(model.trim()) {
        model.clear();
    }
    if base_url.trim() == "https://api.anthropic.com/v1" {
        base_url.clear();
    }
}

fn is_legacy_claude_default(model: &str) -> bool {
    matches!(model, "claude-sonnet-4-6" | "claude-sonnet-4-0")
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

    #[test]
    fn normalize_agent_config_clears_legacy_claude_defaults_for_other_providers() {
        let mut agent = AgentConfig {
            api_key: "x".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            provider: "minimax".to_string(),
            second_provider: String::new(),
            second_api_key: String::new(),
            second_base_url: String::new(),
            second_model: "claude-sonnet-4-0".to_string(),
        };

        normalize_agent_config(&mut agent);

        assert!(agent.base_url.is_empty());
        assert!(agent.model.is_empty());
        assert!(agent.second_model.is_empty());
    }
}
