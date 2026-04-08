/// OpenAI-compatible Chat Completions API loop with SSE streaming.
///
/// POST /chat/completions with Bearer token and `stream: true`.
/// Parses Server-Sent Events for real-time token streaming.
///
/// Compatible with: OpenAI, DeepSeek, Zhipu/GLM, MiniMax, Moonshot,
/// Yi, Baichuan, Qwen, Groq, Together, Fireworks, SiliconFlow, etc.
use super::{emit_chunk, emit_tool_log, MAX_LOOP_ITERATIONS, MAX_RESPONSE_TOKENS};
use crate::errors::{self, AppError};
use crate::tools::{self, ToolRegistry};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub async fn run_loop(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    tool_defs: &[Value],
    workspace: &Path,
    registry: &ToolRegistry,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    read_only: bool,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let oai_tools = tool_defs;

    let mut messages: Vec<Value> = vec![
        json!({ "role": "system", "content": system_prompt }),
        json!({ "role": "user", "content": user_prompt }),
    ];
    let mut full_text = String::new();
    let mut is_first_chunk = true;

    for _iteration in 0..MAX_LOOP_ITERATIONS {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }

        let body = json!({
            "model": model,
            "messages": messages,
            "tools": oai_tools,
            "max_tokens": MAX_RESPONSE_TOKENS,
            "temperature": 0.3,
            "stream": true
        });

        // Parse the SSE stream into a complete response
        let (finish_reason, content, tool_calls) = stream_response(
            client,
            &endpoint,
            api_key,
            &body,
            &token,
            app_handle,
            window_label,
            &mut full_text,
            &mut is_first_chunk,
            subtask_id,
        )
        .await?;

        // Emit tool logs
        for (_, name, input) in &tool_calls {
            emit_tool_log(app_handle, window_label, name, input, registry);
        }

        // Reconstruct assistant message for conversation history
        let mut assistant_msg = json!({ "role": "assistant" });
        if !content.is_empty() {
            assistant_msg["content"] = Value::String(content);
        }
        if !tool_calls.is_empty() {
            let calls: Vec<Value> = tool_calls
                .iter()
                .map(|(id, name, input)| {
                    json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(input).unwrap_or_default()
                        }
                    })
                })
                .collect();
            assistant_msg["tool_calls"] = Value::Array(calls);
        }
        messages.push(assistant_msg);

        if finish_reason != "tool_calls" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        // Execute tools via the registry-based partitioned orchestration
        let results =
            tools::run_partitioned(registry, &tool_calls, workspace, &token, read_only).await?;
        for result in &results {
            let tool_call_id = result["tool_use_id"].as_str().unwrap_or("");
            let content = result["content"].as_str().unwrap_or("");
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": content
            }));
        }
    }

    Ok(full_text)
}

/// Stream an OpenAI-compatible SSE response, emitting chunks in real-time.
///
/// Returns (finish_reason, content_text, tool_calls) on success.
/// Retries on transient errors (429, 5xx, network).
async fn stream_response(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    body: &Value,
    token: &CancellationToken,
    app_handle: &tauri::AppHandle,
    window_label: &str,
    full_text: &mut String,
    is_first_chunk: &mut bool,
    subtask_id: Option<&str>,
) -> Result<(String, String, Vec<(String, String, Value)>), String> {
    // Use retry wrapper for transient failures
    let resp = {
        let client = client.clone();
        let endpoint = endpoint.to_string();
        let api_key = api_key.to_string();
        let body = body.clone();

        errors::with_retry(|| {
            let client = client.clone();
            let endpoint = endpoint.clone();
            let api_key = api_key.clone();
            let body = body.clone();
            async move {
                let resp = client
                    .post(&endpoint)
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .json(&body)
                    .send()
                    .await
                    .map_err(AppError::from)?;

                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(AppError::from_api_status(status, text));
                }
                Ok(resp)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    };

    // Parse SSE stream
    let mut content_text = String::new();
    let mut finish_reason = String::from("stop");

    // Tool call accumulation: index -> (id, name, arguments_json_string)
    let mut tool_call_map: std::collections::HashMap<u64, (String, String, String)> =
        std::collections::HashMap::new();

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    loop {
        let chunk = tokio::select! {
            _ = token.cancelled() => {
                return Err("cancelled".to_string());
            }
            chunk = stream.next() => chunk,
        };

        let chunk = match chunk {
            Some(Ok(bytes)) => bytes,
            Some(Err(e)) => return Err(format!("stream error: {e}")),
            None => break,
        };

        buf.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines
        while let Some(line_end) = buf.find('\n') {
            let line = buf[..line_end].trim_end_matches('\r').to_string();
            buf = buf[line_end + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            let data = match line.strip_prefix("data: ") {
                Some(d) => d,
                None => continue,
            };

            if data == "[DONE]" {
                break;
            }

            let event: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // OpenAI streaming format: choices[0].delta
            let choice = &event["choices"][0];
            let delta = &choice["delta"];

            // Check finish_reason
            if let Some(fr) = choice["finish_reason"].as_str() {
                finish_reason = fr.to_string();
            }

            // Accumulate text content
            if let Some(text) = delta["content"].as_str() {
                if !text.is_empty() {
                    content_text.push_str(text);
                    full_text.push_str(text);
                    emit_chunk(app_handle, window_label, text, is_first_chunk, subtask_id);
                }
            }

            // Accumulate tool calls
            if let Some(calls) = delta["tool_calls"].as_array() {
                for call in calls {
                    let idx = call["index"].as_u64().unwrap_or(0);
                    let entry = tool_call_map
                        .entry(idx)
                        .or_insert_with(|| (String::new(), String::new(), String::new()));

                    if let Some(id) = call["id"].as_str() {
                        entry.0 = id.to_string();
                    }
                    if let Some(name) = call["function"]["name"].as_str() {
                        entry.1.push_str(name);
                    }
                    if let Some(args) = call["function"]["arguments"].as_str() {
                        entry.2.push_str(args);
                    }
                }
            }
        }
    }

    // Convert accumulated tool calls to final format
    let mut tool_calls: Vec<(String, String, Value)> = Vec::new();
    let mut indices: Vec<u64> = tool_call_map.keys().copied().collect();
    indices.sort();
    for idx in indices {
        if let Some((id, name, args_str)) = tool_call_map.remove(&idx) {
            let input: Value = serde_json::from_str(&args_str).unwrap_or(json!({}));
            tool_calls.push((id, name, input));
        }
    }

    Ok((finish_reason, content_text, tool_calls))
}
