/// Provider registry — default endpoints, auth headers, and wire format
/// for each supported LLM provider.
///
/// Adding a new provider:
///   1. Add a variant to the match in `from_name()`
///   2. Set its default base_url and wire format
///   That's it — the loops in anthropic.rs / openai.rs handle the rest.

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

    fn resolve(config: &crate::config::AppConfig, use_second_model: bool) -> Self {
        let agent = &config.agent;

        if agent.is_configured() {
            let info = provider_defaults(&agent.provider);
            let base_url = if agent.base_url.is_empty() {
                info.default_base_url.to_string()
            } else {
                agent.base_url.clone()
            };
            // For review phases, prefer second_model; fall back to model.
            let model_source = if use_second_model && !agent.second_model.is_empty() {
                &agent.second_model
            } else {
                &agent.model
            };
            let model = if model_source.is_empty() {
                info.default_model.to_string()
            } else {
                model_source.clone()
            };
            Self {
                name: agent.provider.to_lowercase(),
                base_url,
                api_key: agent.api_key.clone(),
                model,
                wire: info.wire,
            }
        } else {
            // Fall back to director config
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
            default_model: "claude-sonnet-4-6",
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

        // ── Fireworks AI ────────────────────────────────────────────────────
        "fireworks" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.fireworks.ai/inference/v1",
            default_model: "accounts/fireworks/models/llama-v3p1-70b-instruct",
        },

        // ── SiliconFlow (硅基流动) ──────────────────────────────────────────
        "siliconflow" => ProviderInfo {
            wire: WireFormat::OpenAI,
            default_base_url: "https://api.siliconflow.cn/v1",
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
