/// Provider registry — default endpoints, auth headers, and wire format
/// for each supported LLM provider.
///
/// Adding a new provider:
///   1. Add a variant to the match in `provider_defaults()`
///   2. Set its default base_url and wire format
///   That's it — the loops in anthropic.rs / openai.rs handle the rest.
use serde::Serialize;

/// Wire format: how we talk to the API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireFormat {
    /// Anthropic Messages API: POST /messages, x-api-key header
    Anthropic,
    /// OpenAI Chat Completions API: POST /chat/completions, Bearer token
    OpenAI,
}

/// Resolved provider configuration — everything needed to make API calls.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub wire: WireFormat,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedProviderInfo {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_format: String,
}

impl ProviderConfig {
    /// Build a ProviderConfig from app config, resolving defaults.
    pub fn from_app_config(config: &crate::config::AppConfig) -> Self {
        Self::resolve(config, false)
    }

    /// Build a ProviderConfig for review / read-only phases.
    /// Uses `second_model` if configured, otherwise falls back to `model`.
    pub fn from_app_config_second(config: &crate::config::AppConfig) -> Self {
        Self::resolve(config, true)
    }

    pub fn from_fields(provider: &str, api_key: &str, base_url: &str, model: &str) -> Self {
        let provider = if provider.trim().is_empty() {
            "anthropic"
        } else {
            provider.trim()
        };
        let info = provider_defaults(provider);
        Self {
            name: provider.to_lowercase(),
            base_url: if base_url.trim().is_empty() {
                info.default_base_url.to_string()
            } else {
                base_url.trim().to_string()
            },
            api_key: api_key.trim().to_string(),
            model: if model.trim().is_empty() {
                info.default_model.to_string()
            } else {
                model.trim().to_string()
            },
            wire: info.wire,
        }
    }

    pub fn to_resolved_info(&self) -> ResolvedProviderInfo {
        ResolvedProviderInfo {
            provider: self.name.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_format: match self.wire {
                WireFormat::Anthropic => "anthropic",
                WireFormat::OpenAI => "openai",
            }
            .to_string(),
        }
    }

    fn resolve(config: &crate::config::AppConfig, use_second: bool) -> Self {
        let agent = &config.agent;

        if agent.is_configured() {
            let eff_provider = if use_second && !agent.second_provider.is_empty() {
                &agent.second_provider
            } else {
                &agent.provider
            };
            let eff_api_key = if use_second && !agent.second_api_key.is_empty() {
                &agent.second_api_key
            } else {
                &agent.api_key
            };
            let eff_base_url_raw = if use_second && !agent.second_base_url.is_empty() {
                &agent.second_base_url
            } else {
                &agent.base_url
            };
            let eff_model_raw = if use_second && !agent.second_model.is_empty() {
                &agent.second_model
            } else {
                &agent.model
            };

            let eff_provider = if eff_provider.trim().is_empty() {
                "anthropic"
            } else {
                eff_provider
            };
            let info = provider_defaults(eff_provider);
            let base_url = if eff_base_url_raw.is_empty() {
                info.default_base_url.to_string()
            } else {
                eff_base_url_raw.clone()
            };
            let model = if eff_model_raw.is_empty() {
                info.default_model.to_string()
            } else {
                eff_model_raw.clone()
            };
            Self {
                name: eff_provider.to_lowercase(),
                base_url,
                api_key: eff_api_key.clone(),
                model,
                wire: info.wire,
            }
        } else {
            let wire = match config.director.api_format {
                crate::config::ApiFormat::OpenAI => WireFormat::OpenAI,
                crate::config::ApiFormat::Anthropic => WireFormat::Anthropic,
            };
            Self {
                name: config.director.api_format.as_str().to_string(),
                base_url: config.director.base_url.clone(),
                api_key: config.director.api_key.clone(),
                model: config.director.model.clone(),
                wire,
            }
        }
    }
}

// ── Provider defaults ───────────────────────────────────────────────────────

struct ProviderInfo {
    wire: WireFormat,
    default_base_url: &'static str,
    default_model: &'static str,
}

fn provider_defaults(name: &str) -> ProviderInfo {
    match name.to_lowercase().as_str() {
        // ── Anthropic ───────────────────────────────────────────────────────
        "anthropic" => ProviderInfo {
            wire: WireFormat::Anthropic,
            default_base_url: "https://api.anthropic.com/v1",
            default_model: "claude-sonnet-4-0",
        },

        // ── OpenAI ──────────────────────────────────────────────────────────
        "openai" | "codex" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o",
        },

        // ── DeepSeek ────────────────────────────────────────────────────────
        "deepseek" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.deepseek.com",
            default_model: "deepseek-chat",
        },

        // ── 智谱 (Zhipu / GLM) ─────────────────────────────────────────────
        "zhipu" | "glm" | "chatglm" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://open.bigmodel.cn/api/paas/v4",
            default_model: "glm-4-plus",
        },

        // ── MiniMax ─────────────────────────────────────────────────────────
        "minimax" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.minimax.chat/v1",
            default_model: "MiniMax-Text-01",
        },

        // ── 月之暗面 (Moonshot / Kimi) ──────────────────────────────────────
        "moonshot" | "kimi" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.moonshot.cn/v1",
            default_model: "moonshot-v1-128k",
        },

        // ── 零一万物 (Yi / 01.AI) ───────────────────────────────────────────
        "yi" | "01ai" | "lingyiwanwu" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.lingyiwanwu.com/v1",
            default_model: "yi-large",
        },

        // ── 百川 (Baichuan) ─────────────────────────────────────────────────
        "baichuan" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.baichuan-ai.com/v1",
            default_model: "Baichuan4",
        },

        // ── 通义千问 (Qwen / DashScope) ─────────────────────────────────────
        "qwen" | "dashscope" | "tongyi" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            default_model: "qwen-max",
        },

        // ── Groq ────────────────────────────────────────────────────────────
        "groq" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.groq.com/openai/v1",
            default_model: "llama-3.3-70b-versatile",
        },

        // ── Together AI ─────────────────────────────────────────────────────
        "together" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.together.xyz/v1",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
        },

        // ── Fireworks AI ───────────────────────────────────────────────────
        "fireworks" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.fireworks.ai/inference/v1",
            default_model: "accounts/fireworks/models/llama-v3p1-70b-instruct",
        },

        // ── SiliconFlow (硅基流动) ──────────────────────────────────────────
        "siliconflow" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.siliconflow.com/v1",
            default_model: "Qwen/Qwen2.5-72B-Instruct",
        },

        // ── Unknown: assume OpenAI-compatible ───────────────────────────────
        _ => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_provider_defaults_from_fields() {
        let config = ProviderConfig::from_fields("siliconflow", "", "", "");
        assert_eq!(config.base_url, "https://api.siliconflow.com/v1");
        assert_eq!(config.model, "Qwen/Qwen2.5-72B-Instruct");
        assert_eq!(config.wire, WireFormat::OpenAI);
    }
}
