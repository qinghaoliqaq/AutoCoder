/// OpenAI-compatible Chat Completions API loop.
///
/// POST /chat/completions with Bearer token.
/// All tools use standard JSON Schema function-calling format.
///
/// Compatible with: OpenAI, DeepSeek, Zhipu/GLM, MiniMax, Moonshot,
/// Yi, Baichuan, Qwen, Groq, Together, Fireworks, SiliconFlow, etc.

use super::{emit_chunk, emit_tool_log, execute, tools, MAX_LOOP_ITERATIONS, MAX_RESPONSE_TOKENS};
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
    tool_defs: &[Value],
    workspace: &Path,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
    read_only: bool,
) -> Result<String, String> {
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let oai_tools = tools::to_openai_functions(tool_defs);

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

                    let value: Value = resp.json().await.map_err(AppError::from)?;
                    Ok(value)
                }
            })
            .await
            .map_err(|e| e.to_string())?
        };

        let choice = &response["choices"][0];
        let message = &choice["message"];
        let finish_reason = choice["finish_reason"].as_str().unwrap_or("stop");

        // Extract text content
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                full_text.push_str(text);
                emit_chunk(app_handle, window_label, text, &mut is_first_chunk);
            }
        }

        // Extract tool calls
        let mut tool_calls: Vec<(String, String, Value)> = Vec::new();
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = call["function"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let args_str = call["function"]["arguments"]
                    .as_str()
                    .unwrap_or("{}");
                let input: Value =
                    serde_json::from_str(args_str).unwrap_or(json!({}));
                emit_tool_log(app_handle, window_label, &name, &input);
                tool_calls.push((id, name, input));
            }
        }

        // Append assistant message in original format
        messages.push(message.clone());

        if finish_reason != "tool_calls" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        // Execute tools and append results in OpenAI format
        let results =
            execute::run_partitioned(&tool_calls, workspace, &token, read_only).await?;
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
