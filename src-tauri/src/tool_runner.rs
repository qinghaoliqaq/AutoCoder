/// Tool-use agent loop with dual wire format support.
///
/// Supports both Anthropic and OpenAI-compatible APIs:
///   - Anthropic: bash/editor use built-in type shorthand (model trained on these)
///   - OpenAI/Codex: all tools use standard JSON Schema (universal compatibility)
///
/// The wire format is detected from config (agent.provider or director.api_format).
///
/// Supported tools (execution is 100% local Rust, free, no CLI needed):
///   - bash                         -> shell command execution
///   - str_replace_based_edit_tool  -> file view/create/edit
///   - grep_search                  -> regex search across files
///   - glob_find                    -> file pattern matching
///
/// Features:
///   - Read-only tools execute concurrently; write tools serial
///   - Large results (>30KB) persisted to disk with preview
///   - Configurable max concurrency

use crate::config::AppConfig;
use crate::skills::{SkillChunk, ToolLog};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

const MAX_LOOP_ITERATIONS: usize = 40;
const MAX_RESPONSE_TOKENS: u32 = 16384;
const MAX_TOOL_CONCURRENCY: usize = 10;
const LARGE_RESULT_THRESHOLD: usize = 30_000;
const LARGE_RESULT_PREVIEW: usize = 2_000;
const MAX_RESULT_CHARS: usize = 50_000;

// ── Wire format ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum WireFormat {
    Anthropic,
    OpenAI,
}

fn detect_wire_format(config: &AppConfig) -> WireFormat {
    let agent = &config.agent;
    if agent.is_configured() {
        match agent.provider.to_lowercase().as_str() {
            "openai" | "codex" | "deepseek" | "groq" | "together" | "fireworks" => {
                WireFormat::OpenAI
            }
            _ => WireFormat::Anthropic,
        }
    } else {
        match config.director.api_format {
            crate::config::ApiFormat::OpenAI => WireFormat::OpenAI,
            crate::config::ApiFormat::Anthropic => WireFormat::Anthropic,
        }
    }
}

// ── Tool schemas ────────────────────────────────────────────────────────────

fn bash_schema() -> Value {
    json!({
        "name": "bash",
        "description": "Execute a shell command and return stdout, stderr, exit code.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" },
                "timeout": { "type": "integer", "description": "Optional timeout in ms (max 600000)" }
            },
            "required": ["command"]
        }
    })
}

fn editor_schema() -> Value {
    json!({
        "name": "str_replace_based_edit_tool",
        "description": "Text editor for viewing, creating, and editing files.\nCommands: view, create, str_replace, insert",
        "input_schema": {
            "type": "object",
            "properties": {
                "command":     { "type": "string", "enum": ["view","create","str_replace","insert"], "description": "Editor command" },
                "path":        { "type": "string", "description": "File path" },
                "file_text":   { "type": "string", "description": "Content for create" },
                "old_str":     { "type": "string", "description": "String to find for str_replace (must be unique)" },
                "new_str":     { "type": "string", "description": "Replacement for str_replace or text for insert" },
                "insert_line": { "type": "integer", "description": "Line number for insert" }
            },
            "required": ["command", "path"]
        }
    })
}

fn grep_schema() -> Value {
    json!({
        "name": "grep_search",
        "description": "Search for a regex pattern across files. Returns matching lines with paths and line numbers.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern" },
                "path":    { "type": "string", "description": "Directory to search (absolute path)" },
                "include": { "type": "string", "description": "File glob filter (e.g. '*.rs')" }
            },
            "required": ["pattern", "path"]
        }
    })
}

fn glob_schema() -> Value {
    json!({
        "name": "glob_find",
        "description": "Find files matching a glob pattern. Returns paths sorted by mtime.",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. 'src/**/*.rs')" },
                "path":    { "type": "string", "description": "Root directory (absolute path)" }
            },
            "required": ["pattern", "path"]
        }
    })
}

/// Build tool definitions for the given wire format.
fn tool_definitions(format: WireFormat) -> Vec<Value> {
    let (bash, editor) = match format {
        WireFormat::Anthropic => (
            // Anthropic built-in types: Claude is specifically trained on these
            json!({ "type": "bash_20250124", "name": "bash" }),
            json!({ "type": "text_editor_20250728", "name": "str_replace_based_edit_tool" }),
        ),
        WireFormat::OpenAI => (bash_schema(), editor_schema()),
    };
    vec![bash, editor, grep_schema(), glob_schema()]
}

/// Convert to OpenAI function-calling format.
fn tools_to_openai_functions(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["input_schema"]
                }
            })
        })
        .collect()
}

/// Returns true if the tool call is read-only and safe to run concurrently.
fn is_read_only_tool(name: &str, input: &Value) -> bool {
    match name {
        "grep_search" | "glob_find" => true,
        "str_replace_based_edit_tool" => input["command"].as_str() == Some("view"),
        "bash" => {
            if let Some(cmd) = input["command"].as_str() {
                let trimmed = cmd.trim();
                let read_prefixes = [
                    "cat ", "head ", "tail ", "less ", "wc ", "file ", "ls ", "ls\n",
                    "pwd", "echo ", "which ", "type ", "find ", "grep ", "rg ", "ag ",
                    "fd ", "git log", "git show", "git diff", "git status",
                    "git branch", "git rev-parse", "git remote", "cargo check",
                    "cargo clippy", "rustc --", "python -c", "node -e", "stat ",
                    "du ", "df ",
                ];
                read_prefixes.iter().any(|p| trimmed.starts_with(p))
            } else {
                false
            }
        }
        _ => false,
    }
}

// ── Large result persistence ────────────────────────────────────────────────

fn result_cache_dir() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("ai-dev-hub").join("tool-results")
}

fn maybe_persist_large_result(result: &str, tool_name: &str) -> String {
    if result.len() <= LARGE_RESULT_THRESHOLD {
        return result.to_string();
    }

    let cache_dir = result_cache_dir();
    if std::fs::create_dir_all(&cache_dir).is_err() {
        return truncate_result(result);
    }

    let ts = chrono::Utc::now().timestamp_millis();
    let filename = format!("{tool_name}_{ts}.txt");
    let path = cache_dir.join(&filename);

    if std::fs::write(&path, result).is_ok() {
        let preview = &result[..result
            .char_indices()
            .nth(LARGE_RESULT_PREVIEW)
            .map(|(i, _)| i)
            .unwrap_or(result.len())];
        format!(
            "{preview}\n\n... [result too large: {} chars, full output saved to {}]",
            result.len(),
            path.display(),
        )
    } else {
        truncate_result(result)
    }
}

fn truncate_result(result: &str) -> String {
    if result.len() > MAX_RESULT_CHARS {
        format!(
            "{}...\n[output truncated at {} chars]",
            &result[..MAX_RESULT_CHARS],
            MAX_RESULT_CHARS
        )
    } else {
        result.to_string()
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Run a tool-use agent loop. Auto-detects wire format from config.
pub async fn run(
    config: &AppConfig,
    system_prompt: &str,
    user_prompt: &str,
    cwd: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<String, String> {
    let wire = detect_wire_format(config);
    let agent = &config.agent;

    let (base_url, api_key, model) = if agent.is_configured() {
        let url = if agent.base_url.is_empty() {
            match wire {
                WireFormat::Anthropic => "https://api.anthropic.com/v1".to_string(),
                WireFormat::OpenAI => "https://api.openai.com/v1".to_string(),
            }
        } else {
            agent.base_url.clone()
        };
        (url, agent.api_key.clone(), agent.model.clone())
    } else {
        (
            config.director.base_url.clone(),
            config.director.api_key.clone(),
            config.director.model.clone(),
        )
    };

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let workspace = cwd
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let tools = tool_definitions(wire);

    match wire {
        WireFormat::Anthropic => {
            run_anthropic_loop(
                &client,
                &base_url,
                &api_key,
                &model,
                system_prompt,
                user_prompt,
                &tools,
                &workspace,
                window_label,
                app_handle,
                token,
            )
            .await
        }
        WireFormat::OpenAI => {
            run_openai_loop(
                &client,
                &base_url,
                &api_key,
                &model,
                system_prompt,
                user_prompt,
                &tools,
                &workspace,
                window_label,
                app_handle,
                token,
            )
            .await
        }
    }
}

// ── Anthropic loop ──────────────────────────────────────────────────────────

async fn run_anthropic_loop(
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

        let resp = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("API error {status}: {text}"));
        }

        let response: Value = resp
            .json()
            .await
            .map_err(|e| format!("API JSON parse error: {e}"))?;

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
            execute_tool_calls_partitioned(&tool_calls, workspace, &token).await?;
        messages.push(json!({ "role": "user", "content": tool_results }));
    }

    Ok(full_text)
}

// ── OpenAI loop ─────────────────────────────────────────────────────────────

async fn run_openai_loop(
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
) -> Result<String, String> {
    let endpoint = format!(
        "{}/chat/completions",
        base_url.trim_end_matches('/')
    );
    let oai_tools = tools_to_openai_functions(tools);

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

        let resp = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("API error {status}: {text}"));
        }

        let response: Value = resp
            .json()
            .await
            .map_err(|e| format!("API JSON parse error: {e}"))?;

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

        // Extract tool calls (OpenAI format)
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

        // OpenAI uses "tool_calls" as finish_reason
        if finish_reason != "tool_calls" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        // Execute tools and append results in OpenAI format
        let results =
            execute_tool_calls_partitioned(&tool_calls, workspace, &token).await?;
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

// ── Emit helpers ────────────────────────────────────────────────────────────

fn emit_chunk(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    text: &str,
    is_first_chunk: &mut bool,
) {
    let reset = *is_first_chunk;
    *is_first_chunk = false;
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "skill-chunk",
        SkillChunk {
            agent: "claude".to_string(),
            text: text.to_string(),
            reset,
        },
    );
}

fn emit_tool_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    name: &str,
    input: &Value,
) {
    let ts = chrono::Utc::now().timestamp_millis() as u64;
    let summary = summarize_tool_input(name, input);
    let _ = app_handle.emit_to(
        EventTarget::webview_window(window_label),
        "tool-log",
        ToolLog {
            agent: "claude".to_string(),
            tool: name.to_string(),
            input: summary,
            timestamp: ts,
        },
    );
}

// ── Partitioned tool execution ──────────────────────────────────────────────

async fn execute_tool_calls_partitioned(
    tool_calls: &[(String, String, Value)],
    workspace: &Path,
    token: &CancellationToken,
) -> Result<Vec<Value>, String> {
    let mut batches: Vec<(bool, Vec<usize>)> = Vec::new();
    for (i, (_id, name, input)) in tool_calls.iter().enumerate() {
        let readonly = is_read_only_tool(name, input);
        if readonly && batches.last().map(|b| b.0).unwrap_or(false) {
            batches.last_mut().unwrap().1.push(i);
        } else {
            batches.push((readonly, vec![i]));
        }
    }

    let mut results: Vec<Value> = vec![Value::Null; tool_calls.len()];

    for (is_readonly, indices) in &batches {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }

        if *is_readonly && indices.len() > 1 {
            // Concurrent execution for read-only batch
            let mut handles = Vec::new();
            for &idx in indices {
                let (id, name, input) = &tool_calls[idx];
                let id = id.clone();
                let name = name.clone();
                let input = input.clone();
                let ws = workspace.to_path_buf();
                handles.push(tokio::spawn(async move {
                    let result = execute_tool(&name, &input, &ws).await;
                    (idx, id, name, result)
                }));
            }

            for chunk in handles.chunks_mut(MAX_TOOL_CONCURRENCY) {
                let chunk_results: Vec<_> =
                    futures::future::join_all(chunk.iter_mut().map(|h| async { h.await }))
                        .await;

                for join_result in chunk_results {
                    let (idx, id, name, result) =
                        join_result.map_err(|e| format!("Tool task join error: {e}"))?;
                    let (output, is_error) = match result {
                        Ok(out) => (out, false),
                        Err(err) => (err, true),
                    };
                    let processed = maybe_persist_large_result(&output, &name);
                    let mut obj = json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": processed,
                    });
                    if is_error {
                        obj["is_error"] = json!(true);
                    }
                    results[idx] = obj;
                }
            }
        } else {
            // Serial execution
            for &idx in indices {
                if token.is_cancelled() {
                    return Err("cancelled".to_string());
                }
                let (id, name, input) = &tool_calls[idx];
                let result = execute_tool(name, input, workspace).await;
                let (output, is_error) = match result {
                    Ok(out) => (out, false),
                    Err(err) => (err, true),
                };
                let processed = maybe_persist_large_result(&output, name);
                let mut obj = json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": processed,
                });
                if is_error {
                    obj["is_error"] = json!(true);
                }
                results[idx] = obj;
            }
        }
    }

    Ok(results)
}

// ── Tool execution ──────────────────────────────────────────────────────────

async fn execute_tool(name: &str, input: &Value, workspace: &Path) -> Result<String, String> {
    match name {
        "bash" => tool_bash(input, workspace).await,
        "str_replace_based_edit_tool" => tool_editor(input, workspace),
        "grep_search" => tool_grep(input, workspace),
        "glob_find" => tool_glob(input, workspace),
        other => Err(format!("Unknown tool: {other}")),
    }
}

async fn tool_bash(input: &Value, workspace: &Path) -> Result<String, String> {
    let command = input["command"]
        .as_str()
        .ok_or("bash: missing 'command' field")?;

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| format!("bash: spawn error: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(&stderr);
    }
    if !output.status.success() {
        result.push_str(&format!(
            "\n[exit code: {}]",
            output.status.code().unwrap_or(-1)
        ));
    }
    if result.is_empty() {
        result = "(no output)".to_string();
    }
    Ok(result)
}

fn tool_editor(input: &Value, workspace: &Path) -> Result<String, String> {
    let command = input["command"]
        .as_str()
        .ok_or("editor: missing 'command' field")?;
    let path_str = input["path"]
        .as_str()
        .ok_or("editor: missing 'path' field")?;
    let path = resolve_path(path_str, workspace);

    match command {
        "view" => {
            let content =
                std::fs::read_to_string(&path).map_err(|e| format!("editor view: {e}"))?;
            let numbered: String = content
                .lines()
                .enumerate()
                .map(|(i, line)| format!("{}\t{}", i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(numbered)
        }
        "create" => {
            let file_text = input["file_text"]
                .as_str()
                .ok_or("editor create: missing 'file_text'")?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("editor create: mkdir error: {e}"))?;
            }
            std::fs::write(&path, file_text)
                .map_err(|e| format!("editor create: write error: {e}"))?;
            Ok(format!("Created {}", path.display()))
        }
        "str_replace" => {
            let old_str = input["old_str"]
                .as_str()
                .ok_or("editor str_replace: missing 'old_str'")?;
            let new_str = input["new_str"]
                .as_str()
                .ok_or("editor str_replace: missing 'new_str'")?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("editor str_replace: read error: {e}"))?;
            let count = content.matches(old_str).count();
            if count == 0 {
                return Err(format!(
                    "editor str_replace: '{}' not found in {}",
                    old_str.chars().take(80).collect::<String>(),
                    path.display()
                ));
            }
            if count > 1 {
                return Err(format!(
                    "editor str_replace: '{}' found {count} times (expected 1) in {}",
                    old_str.chars().take(80).collect::<String>(),
                    path.display()
                ));
            }
            let new_content = content.replacen(old_str, new_str, 1);
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("editor str_replace: write error: {e}"))?;
            Ok(format!("Replaced in {}", path.display()))
        }
        "insert" => {
            let insert_line = input["insert_line"]
                .as_u64()
                .ok_or("editor insert: missing 'insert_line'")? as usize;
            let new_str = input["new_str"]
                .as_str()
                .ok_or("editor insert: missing 'new_str'")?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("editor insert: read error: {e}"))?;
            let mut lines: Vec<&str> = content.lines().collect();
            let insert_at = insert_line.min(lines.len());
            let new_lines: Vec<&str> = new_str.lines().collect();
            for (i, nl) in new_lines.iter().enumerate() {
                lines.insert(insert_at + i, nl);
            }
            let new_content = lines.join("\n") + "\n";
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("editor insert: write error: {e}"))?;
            Ok(format!(
                "Inserted at line {} in {}",
                insert_line,
                path.display()
            ))
        }
        other => Err(format!("editor: unknown command '{other}'")),
    }
}

fn tool_grep(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .ok_or("grep: missing 'pattern'")?;
    let search_path = input["path"].as_str().ok_or("grep: missing 'path'")?;
    let include = input["include"].as_str().unwrap_or("");

    let resolved = resolve_path(search_path, workspace);

    let mut cmd = std::process::Command::new("grep");
    cmd.args(["-rn", "--color=never", "-E", pattern])
        .current_dir(workspace);
    if !include.is_empty() {
        cmd.arg("--include").arg(include);
    }
    cmd.arg(resolved.to_string_lossy().as_ref());

    let output = cmd.output().map_err(|e| format!("grep: {e}"))?;
    let result = String::from_utf8_lossy(&output.stdout).to_string();
    if result.is_empty() {
        Ok("(no matches)".to_string())
    } else {
        let limited: String = result.lines().take(200).collect::<Vec<_>>().join("\n");
        if result.lines().count() > 200 {
            Ok(format!("{limited}\n... [truncated, >200 matches]"))
        } else {
            Ok(limited)
        }
    }
}

fn tool_glob(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .ok_or("glob: missing 'pattern'")?;
    let search_path = input["path"].as_str().ok_or("glob: missing 'path'")?;

    let resolved = resolve_path(search_path, workspace);

    let output = std::process::Command::new("find")
        .arg(resolved.to_string_lossy().as_ref())
        .args(["-name", pattern, "-type", "f"])
        .arg("-maxdepth")
        .arg("8")
        .output()
        .map_err(|e| format!("glob: {e}"))?;

    let result = String::from_utf8_lossy(&output.stdout).to_string();
    if result.is_empty() {
        Ok("(no files found)".to_string())
    } else {
        let limited: String = result.lines().take(200).collect::<Vec<_>>().join("\n");
        Ok(limited)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_path(path_str: &str, workspace: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace.join(p)
    }
}

fn summarize_tool_input(name: &str, input: &Value) -> String {
    match name {
        "bash" => input["command"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(150)
            .collect(),
        "str_replace_based_edit_tool" => {
            let cmd = input["command"].as_str().unwrap_or("");
            let path = input["path"].as_str().unwrap_or("");
            format!("{cmd} {path}")
        }
        "grep_search" => {
            let pattern = input["pattern"].as_str().unwrap_or("");
            let path = input["path"].as_str().unwrap_or("");
            format!("/{pattern}/ in {path}")
        }
        "glob_find" => {
            let pattern = input["pattern"].as_str().unwrap_or("");
            format!("find {pattern}")
        }
        _ => serde_json::to_string(input)
            .unwrap_or_default()
            .chars()
            .take(150)
            .collect(),
    }
}
