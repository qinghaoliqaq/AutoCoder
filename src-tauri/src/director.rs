/// Director module — streaming call with conversation history and skill invocation support.
///
/// The Director LLM handles all user interaction.
/// When a dev task is detected, the model appends to its reply:
///   <invoke skill="plan|code|debug|test|review|qa" task="..." />
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
/// Hard ceiling on messages kept even after compaction (safety net).
const MAX_HISTORY_MESSAGES: usize = 200;
/// Number of recent messages to always keep verbatim (never compacted).
const RECENT_PRESERVE_COUNT: usize = 6;

// ── Entry point ───────────────────────────────────────────────────────────────

/// Send a message to the Director and stream the response token by token.
/// Each token is pushed to the frontend via the "director-chat-chunk" Tauri event.
/// The full response is appended to history after streaming completes.
pub async fn chat_with_director(
    config: &AppConfig,
    prompts: &Prompts,
    user_input: &str,
    histories: &Mutex<HashMap<String, Vec<Value>>>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
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
        let h = histories
            .lock()
            .map_err(|e| format!("History lock error: {e}"))?;
        h.get(window_label).cloned().unwrap_or_default()
    };

    let assistant_reply = match config.director.api_format {
        ApiFormat::OpenAI => {
            stream_openai(
                &client,
                config,
                &prompts.director_chat,
                user_input,
                &history_snapshot,
                window_label,
                app_handle,
                token.clone(),
                token,
            )
            .await?
        }
        ApiFormat::Anthropic => {
            stream_anthropic(
                &client,
                config,
                &prompts.director_chat,
                user_input,
                &history_snapshot,
                window_label,
                app_handle,
                token.clone(),
                token,
            )
            .await?
        }
    };

    // Append this exchange to history
    {
        let mut h = histories
            .lock()
            .map_err(|e| format!("History lock error: {e}"))?;
        let window_history = h.entry(window_label.to_string()).or_default();
        window_history.push(json!({ "role": "user",      "content": user_input      }));
        window_history.push(json!({ "role": "assistant", "content": assistant_reply }));
        // Hard safety ceiling to prevent unbounded growth
        enforce_hard_ceiling(window_history);
    }

    // Token-aware compaction: if history exceeds the context budget, compact older
    // messages into a structured summary via an LLM call.
    let needs_compaction = {
        let h = histories.lock().map_err(|e| format!("History lock error: {e}"))?;
        let window_history = h.get(window_label).cloned().unwrap_or_default();
        estimate_tokens(&window_history) > config.director.context_budget
    };

    if needs_compaction {
        compact_history(config, prompts, histories, window_label).await?;
    }

    Ok(())
}

/// Hard safety ceiling: if history somehow exceeds MAX_HISTORY_MESSAGES,
/// fall back to draining the oldest entries. This is a last-resort guard —
/// normal trimming is handled by `compact_history`.
pub(crate) fn enforce_hard_ceiling(history: &mut Vec<Value>) {
    if history.len() > MAX_HISTORY_MESSAGES {
        let overflow = history.len() - MAX_HISTORY_MESSAGES;
        history.drain(0..overflow);
    }
}

// ── Token estimation ─────────────────────────────────────────────────────────

/// Rough token estimate for a message list.
/// Uses ~4 chars/token for ASCII, ~2 chars/token for CJK — a practical approximation
/// that avoids pulling in a full tokenizer dependency.
pub(crate) fn estimate_tokens(messages: &[Value]) -> usize {
    messages.iter().map(|m| {
        let text = m["content"].as_str().unwrap_or("");
        estimate_text_tokens(text)
    }).sum()
}

fn estimate_text_tokens(text: &str) -> usize {
    let mut ascii_chars: usize = 0;
    let mut cjk_chars: usize = 0;
    for c in text.chars() {
        if c > '\u{2E7F}' {
            cjk_chars += 1;
        } else {
            ascii_chars += 1;
        }
    }
    // ~4 ASCII chars per token, ~2 CJK chars per token, minimum 1
    (ascii_chars / 4 + cjk_chars / 2).max(1)
}

// ── Context compaction ───────────────────────────────────────────────────────

/// Compact older messages into a structured summary, preserving recent ones.
///
/// Strategy (inspired by Claude Code's compaction):
///   1. Always keep the most recent RECENT_PRESERVE_COUNT messages verbatim
///   2. Summarize everything before that into a single "[Context Summary]" message
///   3. The summary is generated by the Director LLM itself using compact_summary prompt
///   4. Replace old messages with the summary message
async fn compact_history(
    config:    &AppConfig,
    prompts:   &crate::prompts::Prompts,
    histories: &Mutex<HashMap<String, Vec<Value>>>,
    window_label: &str,
) -> Result<(), String> {
    let snapshot = {
        let h = histories.lock().map_err(|e| format!("History lock error: {e}"))?;
        h.get(window_label).cloned().unwrap_or_default()
    };

    if snapshot.len() <= RECENT_PRESERVE_COUNT {
        return Ok(()); // Nothing to compact
    }

    let split_at = snapshot.len() - RECENT_PRESERVE_COUNT;
    let old_messages = &snapshot[..split_at];
    let recent_messages = &snapshot[split_at..];

    // Build a text representation of old messages for summarization
    let mut conversation_text = String::new();
    for msg in old_messages {
        let role = msg["role"].as_str().unwrap_or("unknown");
        let content = msg["content"].as_str().unwrap_or("");
        // Truncate very long individual messages to keep the summary prompt reasonable
        let truncated = if content.len() > 2000 {
            format!("{}... [truncated]", &content[..2000])
        } else {
            content.to_string()
        };
        conversation_text.push_str(&format!("[{role}]: {truncated}\n\n"));
    }

    let summary_prompt = crate::prompts::Prompts::render(
        &prompts.compact_summary,
        &[("conversation", &conversation_text)],
    );

    // Use a non-streaming LLM call to generate the summary
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let summary = call_llm_sync(&client, config, &summary_prompt).await
        .unwrap_or_else(|_| {
            // If the LLM call fails, fall back to a simple extraction of the first user message
            let first_msg = old_messages.first()
                .and_then(|m| m["content"].as_str())
                .unwrap_or("(conversation start)")
                .chars().take(500).collect::<String>();
            format!("[Context Summary — LLM compaction failed, preserving first message]\n{first_msg}")
        });

    // Replace history: summary message + recent messages
    let mut compacted = vec![
        json!({ "role": "assistant", "content": format!("[Context Summary]\n{summary}") }),
    ];
    compacted.extend_from_slice(recent_messages);

    {
        let mut h = histories.lock().map_err(|e| format!("History lock error: {e}"))?;
        h.insert(window_label.to_string(), compacted);
    }
    Ok(())
}

/// Non-streaming LLM call for internal use (compaction, naming, etc.).
/// Sends a single user message and returns the full response text.
async fn call_llm_sync(
    client: &Client,
    config: &AppConfig,
    prompt: &str,
) -> Result<String, String> {
    match config.director.api_format {
        ApiFormat::OpenAI => {
            let endpoint = format!("{}/chat/completions", config.director.base_url.trim_end_matches('/'));
            let body = json!({
                "model":       config.director.model,
                "messages":    [{ "role": "user", "content": prompt }],
                "temperature": 0.3,
                "max_tokens":  1024,
                "stream":      false
            });
            let resp = send_and_check(client, &endpoint,
                &[("Authorization", &format!("Bearer {}", config.director.api_key))],
                &body).await?;
            let v: Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;
            v["choices"][0]["message"]["content"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "No content in compaction response".to_string())
        }
        ApiFormat::Anthropic => {
            let endpoint = format!("{}/messages", config.director.base_url.trim_end_matches('/'));
            let body = json!({
                "model":       config.director.model,
                "messages":    [{ "role": "user", "content": prompt }],
                "max_tokens":  1024,
                "temperature": 0.3
            });
            let resp = send_and_check(client, &endpoint,
                &[("x-api-key", config.director.api_key.as_str()),
                  ("anthropic-version", "2023-06-01")],
                &body).await?;
            let v: Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;
            v["content"][0]["text"].as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "No content in compaction response".to_string())
        }
    }
}

/// Clear conversation history for a specific window (e.g., when the user starts a new session).
pub fn clear_history(histories: &Mutex<HashMap<String, Vec<Value>>>, window_label: &str) {
    if let Ok(mut h) = histories.lock() {
        h.remove(window_label);
    }
}

/// Return a snapshot of the conversation history for a specific window.
pub fn get_history(
    histories: &Mutex<HashMap<String, Vec<Value>>>,
    window_label: &str,
) -> Vec<Value> {
    histories
        .lock()
        .map(|h| h.get(window_label).cloned().unwrap_or_default())
        .unwrap_or_default()
}

/// Replace the conversation history for a specific window (used when restoring a saved session).
pub fn set_history(
    histories: &Mutex<HashMap<String, Vec<Value>>>,
    window_label: &str,
    mut new_history: Vec<Value>,
) {
    trim_history(&mut new_history);
    if let Ok(mut h) = histories.lock() {
        h.insert(window_label.to_string(), new_history);
    }
}

// ── OpenAI SSE streaming ──────────────────────────────────────────────────────

async fn stream_openai(
    client: &Client,
    config: &AppConfig,
    system_prompt: &str,
    user_msg: &str,
    history: &[Value],
    window_label: &str,
    app_handle: &tauri::AppHandle,
    request_token: CancellationToken,
    token: CancellationToken,
) -> Result<String, String> {
    let endpoint = format!(
        "{}/chat/completions",
        config.director.base_url.trim_end_matches('/')
    );

    // system role (for APIs that support it) + seed exchange (fallback for APIs that ignore system role)
    // Many self-hosted OpenAI-compatible endpoints silently ignore "role: system",
    // so we also inject the prompt as the first user/assistant turn.
    let seed_user =
        format!("[指令]\n{system_prompt}\n[/指令]\n\n以上是你的完整行为规则，请严格遵守。");
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
        &[(
            "Authorization",
            &format!("Bearer {}", config.director.api_key),
        )],
        &body,
        request_token,
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
    client: &Client,
    config: &AppConfig,
    system_prompt: &str,
    user_msg: &str,
    history: &[Value],
    window_label: &str,
    app_handle: &tauri::AppHandle,
    request_token: CancellationToken,
    token: CancellationToken,
) -> Result<String, String> {
    let endpoint = format!(
        "{}/messages",
        config.director.base_url.trim_end_matches('/')
    );

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
            ("x-api-key", config.director.api_key.as_str()),
            ("anthropic-version", "2023-06-01"),
        ],
        &body,
        request_token,
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
    token: CancellationToken,
) -> Result<reqwest::Response, String> {
    let mut req = client
        .post(endpoint)
        .header("Content-Type", "application/json");

    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }

    let resp = tokio::select! {
        _ = token.cancelled() => return Err("cancelled".to_string()),
        result = req.json(body).send() => result.map_err(|e| format!("Request failed: {e}"))?,
    };

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
    fn enforce_hard_ceiling_noop_when_within_limit() {
        let mut h = make_history(MAX_HISTORY_MESSAGES);
        enforce_hard_ceiling(&mut h);
        assert_eq!(h.len(), MAX_HISTORY_MESSAGES);
    }

    #[test]
    fn enforce_hard_ceiling_removes_oldest_when_over_limit() {
        let mut h = make_history(MAX_HISTORY_MESSAGES + 4);
        enforce_hard_ceiling(&mut h);
        assert_eq!(h.len(), MAX_HISTORY_MESSAGES);
        assert_eq!(h[0]["content"].as_str().unwrap(), "4");
    }

    #[test]
    fn enforce_hard_ceiling_noop_on_empty() {
        let mut h: Vec<Value> = vec![];
        enforce_hard_ceiling(&mut h);
        assert!(h.is_empty());
    }

    // ── Token estimation ─────────────────────────────────────────────────────

    #[test]
    fn estimate_tokens_ascii() {
        // "hello world" = 11 chars → ~2-3 tokens
        let msgs = vec![json!({ "role": "user", "content": "hello world" })];
        let est = estimate_tokens(&msgs);
        assert!(est >= 1 && est <= 5);
    }

    #[test]
    fn estimate_tokens_cjk() {
        // 4 CJK chars → ~2 tokens
        let msgs = vec![json!({ "role": "user", "content": "你好世界" })];
        let est = estimate_tokens(&msgs);
        assert!(est >= 1 && est <= 4);
    }

    #[test]
    fn estimate_tokens_mixed() {
        let msgs = vec![
            json!({ "role": "user", "content": "Hello 你好" }),
            json!({ "role": "assistant", "content": "World 世界" }),
        ];
        let est = estimate_tokens(&msgs);
        assert!(est >= 2);
    }

    #[test]
    fn estimate_tokens_empty_content() {
        let msgs = vec![json!({ "role": "user", "content": "" })];
        let est = estimate_tokens(&msgs);
        assert_eq!(est, 1); // minimum is 1
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
