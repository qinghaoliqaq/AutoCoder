/// Director module — streaming call with conversation history and skill invocation support.
///
/// The Director LLM handles all user interaction.
/// When a dev task is detected, the model appends to its reply:
///   <invoke skill="plan|code|debug|test" task="..." />
/// The frontend parses this tag and routes to the appropriate skill.
///
/// Supports two API wire formats (set api_format in config.toml):
///   "openai"    → POST {base_url}/chat/completions, Authorization: Bearer
///   "anthropic" → POST {base_url}/messages,         x-api-key header

use crate::config::{ApiFormat, AppConfig};
use crate::prompts::Prompts;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

const MAX_TOKENS: u32 = 2048;
/// Max conversation turns to keep in history (each turn = 1 user + 1 assistant message).
const MAX_HISTORY_TURNS: usize = 20;

// ── Entry point ───────────────────────────────────────────────────────────────

/// Send a message to the Director and stream the response token by token.
/// Each token is pushed to the frontend via the "director-chat-chunk" Tauri event.
/// The full response is appended to history after streaming completes.
pub async fn chat_with_director(
    config:       &AppConfig,
    prompts:      &Prompts,
    user_input:   &str,
    histories:    &Mutex<HashMap<String, Vec<Value>>>,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    if !config.is_configured() {
        return Err(
            "Director not configured. Set DIRECTOR_API_KEY and DIRECTOR_BASE_URL \
             in config.toml or via environment variables."
                .to_string(),
        );
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Snapshot current history for this window (avoid holding the lock during I/O)
    let history_snapshot = {
        let h = histories.lock().map_err(|e| format!("History lock error: {e}"))?;
        h.get(window_label).cloned().unwrap_or_default()
    };

    let assistant_reply = match config.director.api_format {
        ApiFormat::OpenAI => {
            stream_openai(&client, config, &prompts.director_chat, user_input, &history_snapshot, window_label, app_handle, token).await?
        }
        ApiFormat::Anthropic => {
            stream_anthropic(&client, config, &prompts.director_chat, user_input, &history_snapshot, window_label, app_handle, token).await?
        }
    };

    // Append this exchange to history, trimming to the rolling window
    {
        let mut h = histories.lock().map_err(|e| format!("History lock error: {e}"))?;
        let window_history = h.entry(window_label.to_string()).or_default();
        window_history.push(json!({ "role": "user",      "content": user_input      }));
        window_history.push(json!({ "role": "assistant", "content": assistant_reply }));
        trim_history(window_history);
    }

    Ok(())
}

/// Enforce the rolling-window limit on a history vec (mutates in place).
/// Exported so it can be unit-tested without spawning HTTP connections.
pub(crate) fn trim_history(history: &mut Vec<Value>) {
    let max_msgs = MAX_HISTORY_TURNS * 2;
    if history.len() > max_msgs {
        let overflow = history.len() - max_msgs;
        history.drain(0..overflow);
    }
}

/// Clear conversation history for a specific window (e.g., when the user starts a new session).
pub fn clear_history(histories: &Mutex<HashMap<String, Vec<Value>>>, window_label: &str) {
    if let Ok(mut h) = histories.lock() {
        h.remove(window_label);
    }
}

/// Return a snapshot of the conversation history for a specific window.
pub fn get_history(histories: &Mutex<HashMap<String, Vec<Value>>>, window_label: &str) -> Vec<Value> {
    histories.lock()
        .map(|h| h.get(window_label).cloned().unwrap_or_default())
        .unwrap_or_default()
}

/// Replace the conversation history for a specific window (used when restoring a saved session).
pub fn set_history(histories: &Mutex<HashMap<String, Vec<Value>>>, window_label: &str, new_history: Vec<Value>) {
    if let Ok(mut h) = histories.lock() {
        h.insert(window_label.to_string(), new_history);
    }
}

// ── OpenAI SSE streaming ──────────────────────────────────────────────────────

async fn stream_openai(
    client:       &Client,
    config:       &AppConfig,
    system_prompt: &str,
    user_msg:     &str,
    history:      &[Value],
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let endpoint = format!("{}/chat/completions", config.director.base_url.trim_end_matches('/'));

    // system role (for APIs that support it) + seed exchange (fallback for APIs that ignore system role)
    // Many self-hosted OpenAI-compatible endpoints silently ignore "role: system",
    // so we also inject the prompt as the first user/assistant turn.
    let seed_user = format!(
        "[指令]\n{system_prompt}\n[/指令]\n\n以上是你的完整行为规则，请严格遵守。"
    );
    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt }),
        json!({ "role": "user",      "content": seed_user }),
        json!({ "role": "assistant", "content": "收到，我是 Director，将严格按照以上规则行动。" }),
    ];
    messages.extend_from_slice(history);
    messages.push(json!({ "role": "user", "content": user_msg }));

    let body = json!({
        "model":       config.director.model,
        "messages":    messages,
        "temperature": 0.7,
        "max_tokens":  MAX_TOKENS,
        "stream":      true
    });

    let resp = send_and_check(
        client,
        &endpoint,
        &[("Authorization", &format!("Bearer {}", config.director.api_key))],
        &body,
    )
    .await?;

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut full_response = String::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => return Err("cancelled".to_string()),
            chunk = stream.next() => {
                let Some(item) = chunk else { break };
                let bytes = item.map_err(|e| format!("Stream read error: {e}"))?;
                buf.push_str(&String::from_utf8_lossy(&bytes));

                loop {
                    match buf.find('\n') {
                        None => break,
                        Some(pos) => {
                            let line = buf[..pos].trim_end_matches('\r').to_string();
                            buf = buf[pos + 1..].to_string();

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" {
                                    return Ok(full_response);
                                }
                                if let Ok(v) = serde_json::from_str::<Value>(data) {
                                    if let Some(tok) = v["choices"][0]["delta"]["content"].as_str() {
                                        if !tok.is_empty() {
                                            full_response.push_str(tok);
                                            app_handle
                                                .emit_to(
                                                    EventTarget::webview_window(window_label),
                                                    "director-chat-chunk",
                                                    tok.to_string(),
                                                )
                                                .map_err(|e| format!("Emit error: {e}"))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(full_response)
}

// ── Anthropic SSE streaming ───────────────────────────────────────────────────

async fn stream_anthropic(
    client:       &Client,
    config:       &AppConfig,
    system_prompt: &str,
    user_msg:     &str,
    history:      &[Value],
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let endpoint = format!("{}/messages", config.director.base_url.trim_end_matches('/'));

    // history + current user message (system prompt is a top-level field in Anthropic format)
    let mut messages: Vec<Value> = history.to_vec();
    messages.push(json!({ "role": "user", "content": user_msg }));

    let body = json!({
        "model":       config.director.model,
        "system":      system_prompt,
        "messages":    messages,
        "max_tokens":  MAX_TOKENS,
        "temperature": 0.7,
        "stream":      true
    });

    let resp = send_and_check(
        client,
        &endpoint,
        &[
            ("x-api-key",         config.director.api_key.as_str()),
            ("anthropic-version", "2023-06-01"),
        ],
        &body,
    )
    .await?;

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut full_response = String::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => return Err("cancelled".to_string()),
            chunk = stream.next() => {
                let Some(item) = chunk else { break };
                let bytes = item.map_err(|e| format!("Stream read error: {e}"))?;
                buf.push_str(&String::from_utf8_lossy(&bytes));

                loop {
                    match buf.find('\n') {
                        None => break,
                        Some(pos) => {
                            let line = buf[..pos].trim_end_matches('\r').to_string();
                            buf = buf[pos + 1..].to_string();

                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Ok(v) = serde_json::from_str::<Value>(data) {
                                    if v["type"] == "content_block_delta" {
                                        if let Some(tok) = v["delta"]["text"].as_str() {
                                            if !tok.is_empty() {
                                                full_response.push_str(tok);
                                                app_handle
                                                    .emit_to(
                                                        EventTarget::webview_window(window_label),
                                                        "director-chat-chunk",
                                                        tok.to_string(),
                                                    )
                                                    .map_err(|e| format!("Emit error: {e}"))?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(full_response)
}

// ── HTTP helper ───────────────────────────────────────────────────────────────

async fn send_and_check(
    client: &Client,
    endpoint: &str,
    extra_headers: &[(&str, &str)],
    body: &Value,
) -> Result<reqwest::Response, String> {
    let mut req = client
        .post(endpoint)
        .header("Content-Type", "application/json");

    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }

    let resp = req
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(format!("API error {status}: {text}"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_history(n: usize) -> Vec<Value> {
        (0..n).map(|i| json!({ "role": if i % 2 == 0 { "user" } else { "assistant" }, "content": i.to_string() })).collect()
    }

    #[test]
    fn trim_history_noop_when_within_limit() {
        let mut h = make_history(MAX_HISTORY_TURNS * 2);
        trim_history(&mut h);
        assert_eq!(h.len(), MAX_HISTORY_TURNS * 2);
    }

    #[test]
    fn trim_history_removes_oldest_when_over_limit() {
        let limit = MAX_HISTORY_TURNS * 2;
        let mut h = make_history(limit + 4);
        trim_history(&mut h);
        assert_eq!(h.len(), limit);
        // Oldest messages (index 0–3) should be gone; first remaining should be msg #4
        assert_eq!(h[0]["content"].as_str().unwrap(), "4");
    }

    #[test]
    fn trim_history_noop_on_empty() {
        let mut h: Vec<Value> = vec![];
        trim_history(&mut h);
        assert!(h.is_empty());
    }

    #[test]
    fn clear_history_removes_window_entry() {
        let histories = Mutex::new(HashMap::new());
        {
            let mut h = histories.lock().unwrap();
            h.insert("win1".to_string(), make_history(4));
        }
        clear_history(&histories, "win1");
        assert!(get_history(&histories, "win1").is_empty());
    }

    #[test]
    fn set_and_get_history_round_trip() {
        let histories = Mutex::new(HashMap::new());
        let data = make_history(6);
        set_history(&histories, "win2", data.clone());
        let got = get_history(&histories, "win2");
        assert_eq!(got.len(), 6);
        assert_eq!(got[0]["content"], data[0]["content"]);
    }
}
