use crate::config::AppConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Whether the agent API is configured (api_key present in [agent] or [director]).
    pub api_configured: bool,
    /// Provider name (e.g. "anthropic", "openai", "deepseek").
    pub api_provider: String,
    /// Model configured for the agent layer.
    pub api_model: String,
}

pub fn detect_tools() -> SystemStatus {
    let config = AppConfig::load();

    let api_configured = config.agent.is_configured() || config.is_configured();
    let api_provider = if config.agent.is_configured() {
        config.agent.provider.clone()
    } else {
        format!("{:?}", config.director.api_format).to_lowercase()
    };
    let api_model = if config.agent.is_configured() {
        config.agent.model.clone()
    } else {
        config.director.model.clone()
    };

    SystemStatus {
        api_configured,
        api_provider,
        api_model,
    }
}
