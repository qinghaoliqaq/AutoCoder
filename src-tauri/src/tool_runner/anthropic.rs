use super::errors::{self, AppError};
/// Anthropic Messages API loop with SSE streaming.
///
/// POST /messages with x-api-key header and `stream: true`.
/// Parses Server-Sent Events for real-time token streaming.
/// Bash and editor use Anthropic built-in tool type shorthand.
use super::{
    emit_chunk, emit_token_usage, emit_tool_log, CONTEXT_BUDGET_TOKENS, MAX_LOOP_ITERATIONS,
    MAX_RESPONSE_TOKENS, PRUNE_THRESHOLD,
};
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
    tools: &[Value],
    workspace: &Path,
    registry: &ToolRegistry,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    read_only: bool,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    let endpoint = format!("{}/messages", base_url.trim_end_matches('/'));
    let mut messages: Vec<Value> = vec![json!({ "role": "user", "content": user_prompt })];
    let mut full_text = String::new();
    let mut is_first_chunk = true;

    for _iteration in 0..MAX_LOOP_ITERATIONS {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }

        let body = json!({
            "model": model,
            "system": system_prompt,
            "messages": messages,
            "tools": tools,
            "max_tokens": MAX_RESPONSE_TOKENS,
            "temperature": 0.3,
            "stream": true
        });

        // Parse the SSE stream into a complete response
        let (stop_reason, content_blocks, in_tok, out_tok) = stream_response(
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

        // Emit token usage
        emit_token_usage(app_handle, window_label, in_tok, out_tok, subtask_id);

        // Collect tool calls from content blocks
        let mut tool_calls: Vec<(String, String, Value)> = Vec::new();
        for block in &content_blocks {
            if block["type"].as_str() == Some("tool_use") {
                let id = block["id"].as_str().unwrap_or("").to_string();
                let name = block["name"].as_str().unwrap_or("").to_string();
                let input = block["input"].clone();
                emit_tool_log(app_handle, window_label, &name, &input, registry);
                tool_calls.push((id, name, input));
            }
        }

        messages.push(json!({ "role": "assistant", "content": content_blocks }));

        if stop_reason != "tool_use" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        // Execute tools via the registry-based partitioned orchestration
        let tool_results =
            tools::run_partitioned(registry, &tool_calls, workspace, &token, read_only).await?;
        messages.push(json!({ "role": "user", "content": tool_results }));

        // ── Prune old tool-use rounds if context is growing too large ──────
        // messages[0] is always the initial user prompt — keep it.
        // Subsequent messages are (assistant, user) pairs from tool-use rounds.
        let threshold = (CONTEXT_BUDGET_TOKENS as f64 * PRUNE_THRESHOLD) as u64;
        if in_tok > threshold && messages.len() > 3 {
            let round_count = (messages.len() - 1) / 2;
            let rounds_to_remove = round_count / 2;
            if rounds_to_remove > 0 {
                let msgs_to_remove = rounds_to_remove * 2;
                tracing::warn!(
                    "Context budget {:.0}% full ({in_tok} tokens) — pruning {rounds_to_remove} oldest tool-use rounds",
                    (in_tok as f64 / CONTEXT_BUDGET_TOKENS as f64) * 100.0
                );
                messages.drain(1..1 + msgs_to_remove);
                // Validate: the Anthropic API requires strict user/assistant
                // alternation starting with "user".  If pruning left the
                // messages in an invalid state (e.g. two consecutive roles),
                // drop messages from the front until alternation is restored.
                while messages.len() > 1 && messages[0]["role"].as_str() != Some("user") {
                    messages.remove(0);
                }
            }
        }
    }

    Ok(full_text)
}

/// Stream an Anthropic SSE response, emitting chunks in real-time.
///
/// Returns (stop_reason, content_blocks, input_tokens, output_tokens) on success.
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
) -> Result<(String, Vec<Value>, u64, u64), String> {
    // Use retry wrapper for transient failures
    let resp = {
        let client = client.clone();
        let endpoint = endpoint.to_string();
        let api_key = api_key.to_string();
        let body = body.clone();

        errors::with_retry(
            || {
                let client = client.clone();
                let endpoint = endpoint.clone();
                let api_key = api_key.clone();
                let body = body.clone();
                async move {
                    let resp = client
                        .post(&endpoint)
                        .header("Content-Type", "application/json")
                        .header("x-api-key", &api_key)
                        .header("anthropic-version", "2023-06-01")
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
            },
            Some(token),
        )
        .await
        .map_err(|e| e.to_string())?
    };

    // Parse SSE stream
    let mut content_blocks: Vec<Value> = Vec::new();
    let mut stop_reason = String::from("end_turn");
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;

    // Track in-progress content block for accumulating deltas
    let mut current_block_index: Option<usize> = None;
    let mut current_text = String::new();
    let mut current_input_json = String::new();

    let mut stream = resp.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut stream_done = false;

    loop {
        if stream_done {
            break;
        }
        let chunk = tokio::select! {
            _ = token.cancelled() => {
                return Err("cancelled".to_string());
            }
            chunk = stream.next() => chunk,
        };

        let chunk = match chunk {
            Some(Ok(bytes)) => bytes,
            Some(Err(e)) => return Err(format!("stream error: {e}")),
            None => break, // Stream ended
        };

        byte_buf.extend_from_slice(&chunk);

        // Process complete SSE lines (split on \n in the byte buffer)
        while let Some(pos) = byte_buf.iter().position(|&b| b == b'\n') {
            let line_bytes = byte_buf[..pos].to_vec();
            byte_buf.drain(..=pos);
            let line = String::from_utf8_lossy(&line_bytes)
                .trim_end_matches('\r')
                .to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            // Parse "event: <type>" and "data: <json>" lines
            if line.starts_with("event: ") {
                // We handle events implicitly via the data payload
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    stream_done = true;
                    break;
                }

                let event: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let event_type = event["type"].as_str().unwrap_or("");

                match event_type {
                    "content_block_start" => {
                        let idx = event["index"].as_u64().unwrap_or(0) as usize;
                        let block = &event["content_block"];
                        let block_type = block["type"].as_str().unwrap_or("text");

                        current_block_index = Some(idx);
                        current_text.clear();
                        current_input_json.clear();

                        // Initialize the block
                        match block_type {
                            "text" => {
                                // Ensure vec is large enough
                                while content_blocks.len() <= idx {
                                    content_blocks.push(json!(null));
                                }
                                content_blocks[idx] = json!({
                                    "type": "text",
                                    "text": ""
                                });
                            }
                            "tool_use" => {
                                while content_blocks.len() <= idx {
                                    content_blocks.push(json!(null));
                                }
                                content_blocks[idx] = json!({
                                    "type": "tool_use",
                                    "id": block["id"].as_str().unwrap_or(""),
                                    "name": block["name"].as_str().unwrap_or(""),
                                    "input": {}
                                });
                            }
                            _ => {}
                        }
                    }

                    "content_block_delta" => {
                        let delta = &event["delta"];
                        let delta_type = delta["type"].as_str().unwrap_or("");

                        match delta_type {
                            "text_delta" => {
                                if let Some(text) = delta["text"].as_str() {
                                    current_text.push_str(text);
                                    full_text.push_str(text);
                                    emit_chunk(
                                        app_handle,
                                        window_label,
                                        text,
                                        is_first_chunk,
                                        subtask_id,
                                    );
                                }
                            }
                            "input_json_delta" => {
                                if let Some(json_chunk) = delta["partial_json"].as_str() {
                                    current_input_json.push_str(json_chunk);
                                }
                            }
                            _ => {}
                        }
                    }

                    "content_block_stop" => {
                        if let Some(idx) = current_block_index {
                            if idx < content_blocks.len() {
                                let block_type = content_blocks[idx]["type"].as_str().unwrap_or("");
                                match block_type {
                                    "text" => {
                                        content_blocks[idx]["text"] =
                                            Value::String(current_text.clone());
                                    }
                                    "tool_use" => {
                                        let parsed_input: Value =
                                            serde_json::from_str(&current_input_json)
                                                .unwrap_or(json!({}));
                                        content_blocks[idx]["input"] = parsed_input;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        current_block_index = None;
                        current_text.clear();
                        current_input_json.clear();
                    }

                    "message_start" => {
                        // Extract input token count from message_start.message.usage
                        if let Some(usage) = event["message"]["usage"].as_object() {
                            if let Some(it) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                                input_tokens = it;
                            }
                        }
                    }

                    "message_delta" => {
                        if let Some(sr) = event["delta"]["stop_reason"].as_str() {
                            stop_reason = sr.to_string();
                        }
                        // Extract output token count from message_delta.usage
                        if let Some(usage) = event.get("usage") {
                            if let Some(ot) = usage["output_tokens"].as_u64() {
                                output_tokens = ot;
                            }
                        }
                    }

                    "message_stop" => {
                        // Stream complete
                    }

                    "error" => {
                        let msg = event["error"]["message"]
                            .as_str()
                            .unwrap_or("unknown stream error");
                        return Err(format!("Anthropic stream error: {msg}"));
                    }

                    _ => {
                        // ping, etc. — ignore
                    }
                }
            }
        }
    }

    // Filter out any null placeholders
    content_blocks.retain(|b| !b.is_null());

    Ok((stop_reason, content_blocks, input_tokens, output_tokens))
}
