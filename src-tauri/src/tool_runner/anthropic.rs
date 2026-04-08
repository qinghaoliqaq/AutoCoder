/// Anthropic Messages API loop.
///
/// POST /messages with x-api-key header.
/// Bash and editor use Anthropic built-in tool type shorthand.
use super::{emit_chunk, emit_tool_log, execute, MAX_LOOP_ITERATIONS, MAX_RESPONSE_TOKENS};
use crate::errors::{self, AppError};
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
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    read_only: bool,
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
            "temperature": 0.3
        });

        let response = {
            let client = client.clone();
            let endpoint = endpoint.clone();
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

                    let value: Value = resp.json().await.map_err(AppError::from)?;
                    Ok(value)
                }
            })
            .await
            .map_err(|e| e.to_string())?
        };

        let stop_reason = response["stop_reason"].as_str().unwrap_or("end_turn");
        let content = response["content"]
            .as_array()
            .ok_or("Missing content array in API response")?
            .clone();

        let mut tool_calls: Vec<(String, String, Value)> = Vec::new();

        for block in &content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        if !text.is_empty() {
                            full_text.push_str(text);
                            emit_chunk(app_handle, window_label, text, &mut is_first_chunk);
                        }
                    }
                }
                Some("tool_use") => {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input = block["input"].clone();
                    emit_tool_log(app_handle, window_label, &name, &input);
                    tool_calls.push((id, name, input));
                }
                _ => {}
            }
        }

        messages.push(json!({ "role": "assistant", "content": content }));

        if stop_reason != "tool_use" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        let tool_results =
            execute::run_partitioned(&tool_calls, workspace, &token, read_only).await?;
        messages.push(json!({ "role": "user", "content": tool_results }));
    }

    Ok(full_text)
}
