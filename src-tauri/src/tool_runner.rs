/// Tool-use agent loop — runs Claude with Anthropic API tool_use,
/// executing tools locally in Rust.
///
/// This replaces both the CLI runners (claude/codex) and the sidecar approach.
/// It calls the Anthropic Messages API directly (reusing the HTTP client pattern
/// from director.rs), handles tool_use responses by executing tools in-process,
/// and loops until the model stops requesting tools.
///
/// Supported tools:
///   - bash (Anthropic built-in)        → shell command execution
///   - str_replace_based_edit_tool       → file view/create/edit (Anthropic built-in)
///   - grep_search                       → regex search across files
///   - glob_find                         → file pattern matching

use crate::config::AppConfig;
use crate::skills::{SkillChunk, ToolLog};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

/// Max iterations of the tool-use loop (safety net against infinite loops).
const MAX_LOOP_ITERATIONS: usize = 40;
/// Max tokens for the model's response in each iteration.
const MAX_RESPONSE_TOKENS: u32 = 16384;

// ── Tool definitions ─────────────────────────────────────────────────────────

fn tool_definitions() -> Vec<Value> {
    vec![
        // Anthropic built-in: Bash
        json!({ "type": "bash_20250124", "name": "bash" }),
        // Anthropic built-in: Text Editor
        json!({ "type": "text_editor_20250728", "name": "str_replace_based_edit_tool" }),
        // Custom: grep search
        json!({
            "name": "grep_search",
            "description": "Search for a regex pattern across files in a directory. Returns matching lines with file paths and line numbers. Use this to find function definitions, usages, TODOs, etc.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path":    { "type": "string", "description": "Directory to search in (absolute path)" },
                    "include": { "type": "string", "description": "Glob pattern to filter files (e.g. '*.rs'). Optional." }
                },
                "required": ["pattern", "path"]
            }
        }),
        // Custom: glob find
        json!({
            "name": "glob_find",
            "description": "Find files matching a glob pattern. Returns file paths sorted by modification time. Use this to discover project structure and locate files.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern (e.g. 'src/**/*.rs', '*.toml')" },
                    "path":    { "type": "string", "description": "Root directory to search in (absolute path)" }
                },
                "required": ["pattern", "path"]
            }
        }),
    ]
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Run a tool-use agent loop. Sends the prompt to Claude with tool definitions,
/// executes any tool calls locally, streams text chunks to the frontend, and
/// returns the final accumulated assistant text.
pub async fn run(
    config:       &AppConfig,
    system_prompt: &str,
    user_prompt:  &str,
    cwd:          Option<&str>,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    let agent = &config.agent;
    // Determine API endpoint and auth — prefer [agent] config, fall back to [director]
    let (base_url, api_key, model) = if agent.is_configured() {
        let url = if agent.base_url.is_empty() {
            "https://api.anthropic.com/v1".to_string()
        } else {
            agent.base_url.clone()
        };
        (url, agent.api_key.clone(), agent.model.clone())
    } else {
        // Fall back to director config (must be Anthropic format)
        (config.director.base_url.clone(), config.director.api_key.clone(), config.director.model.clone())
    };

    let endpoint = format!("{}/messages", base_url.trim_end_matches('/'));
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let workspace = cwd.map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let tools = tool_definitions();
    let mut messages: Vec<Value> = vec![
        json!({ "role": "user", "content": user_prompt }),
    ];
    let mut full_text = String::new();
    let mut is_first_chunk = true;

    for _iteration in 0..MAX_LOOP_ITERATIONS {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }

        // ── Call the API ─────────────────────────────────────────────────────
        let body = json!({
            "model":      model,
            "system":     system_prompt,
            "messages":   messages,
            "tools":      tools,
            "max_tokens": MAX_RESPONSE_TOKENS,
            "temperature": 0.3
        });

        let resp = client.post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-api-key", &api_key)
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

        let response: Value = resp.json().await
            .map_err(|e| format!("API JSON parse error: {e}"))?;

        let stop_reason = response["stop_reason"].as_str().unwrap_or("end_turn");
        let content = response["content"].as_array()
            .ok_or("Missing content array in API response")?
            .clone();

        // ── Process content blocks ───────────────────────────────────────────
        let mut tool_calls: Vec<(String, String, Value)> = Vec::new(); // (id, name, input)

        for block in &content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        if !text.is_empty() {
                            full_text.push_str(text);
                            let reset = is_first_chunk;
                            is_first_chunk = false;
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
                    }
                }
                Some("tool_use") => {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input = block["input"].clone();

                    // Emit tool log to frontend
                    let ts = chrono::Utc::now().timestamp_millis() as u64;
                    let summary = summarize_tool_input(&name, &input);
                    let _ = app_handle.emit_to(
                        EventTarget::webview_window(window_label),
                        "tool-log",
                        ToolLog { agent: "claude".to_string(), tool: name.clone(), input: summary, timestamp: ts },
                    );

                    tool_calls.push((id, name, input));
                }
                _ => {}
            }
        }

        // Append assistant response to conversation
        messages.push(json!({ "role": "assistant", "content": content }));

        // ── If no tool calls, we're done ─────────────────────────────────────
        if stop_reason != "tool_use" || tool_calls.is_empty() {
            return Ok(full_text);
        }

        // ── Execute tools and build results ──────────────────────────────────
        let mut tool_results: Vec<Value> = Vec::new();

        for (id, name, input) in tool_calls {
            if token.is_cancelled() {
                return Err("cancelled".to_string());
            }

            let result = execute_tool(&name, &input, &workspace).await;

            let (output, is_error) = match result {
                Ok(out) => (out, false),
                Err(err) => (err, true),
            };

            // Truncate very large outputs to avoid blowing up the context
            let truncated = if output.len() > 50_000 {
                format!("{}...\n[output truncated at 50000 chars]", &output[..50_000])
            } else {
                output
            };

            let mut result_obj = json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": truncated,
            });
            if is_error {
                result_obj["is_error"] = json!(true);
            }
            tool_results.push(result_obj);
        }

        messages.push(json!({ "role": "user", "content": tool_results }));
    }

    Ok(full_text)
}

// ── Tool execution ───────────────────────────────────────────────────────────

async fn execute_tool(name: &str, input: &Value, workspace: &Path) -> Result<String, String> {
    match name {
        "bash"                          => tool_bash(input, workspace).await,
        "str_replace_based_edit_tool"   => tool_editor(input, workspace),
        "grep_search"                   => tool_grep(input, workspace),
        "glob_find"                     => tool_glob(input, workspace),
        other => Err(format!("Unknown tool: {other}")),
    }
}

// ── bash ─────────────────────────────────────────────────────────────────────

async fn tool_bash(input: &Value, workspace: &Path) -> Result<String, String> {
    let command = input["command"].as_str()
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
        if !result.is_empty() { result.push('\n'); }
        result.push_str("[stderr]\n");
        result.push_str(&stderr);
    }
    if !output.status.success() {
        result.push_str(&format!("\n[exit code: {}]", output.status.code().unwrap_or(-1)));
    }
    if result.is_empty() {
        result = "(no output)".to_string();
    }
    Ok(result)
}

// ── str_replace_based_edit_tool (Anthropic built-in schema) ──────────────────

fn tool_editor(input: &Value, workspace: &Path) -> Result<String, String> {
    let command = input["command"].as_str()
        .ok_or("editor: missing 'command' field")?;
    let path_str = input["path"].as_str()
        .ok_or("editor: missing 'path' field")?;
    let path = resolve_path(path_str, workspace);

    match command {
        "view" => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("editor view: {e}"))?;
            // Add line numbers
            let numbered: String = content.lines().enumerate()
                .map(|(i, line)| format!("{}\t{}", i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(numbered)
        }
        "create" => {
            let file_text = input["file_text"].as_str()
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
            let old_str = input["old_str"].as_str()
                .ok_or("editor str_replace: missing 'old_str'")?;
            let new_str = input["new_str"].as_str()
                .ok_or("editor str_replace: missing 'new_str'")?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("editor str_replace: read error: {e}"))?;
            let count = content.matches(old_str).count();
            if count == 0 {
                return Err(format!("editor str_replace: '{}' not found in {}",
                    old_str.chars().take(80).collect::<String>(), path.display()));
            }
            if count > 1 {
                return Err(format!("editor str_replace: '{}' found {count} times (expected 1) in {}",
                    old_str.chars().take(80).collect::<String>(), path.display()));
            }
            let new_content = content.replacen(old_str, new_str, 1);
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("editor str_replace: write error: {e}"))?;
            Ok(format!("Replaced in {}", path.display()))
        }
        "insert" => {
            let insert_line = input["insert_line"].as_u64()
                .ok_or("editor insert: missing 'insert_line'")? as usize;
            let new_str = input["new_str"].as_str()
                .ok_or("editor insert: missing 'new_str'")?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("editor insert: read error: {e}"))?;
            let mut lines: Vec<&str> = content.lines().collect();
            let insert_at = insert_line.min(lines.len());
            // Split new_str into lines and insert
            let new_lines: Vec<&str> = new_str.lines().collect();
            for (i, nl) in new_lines.iter().enumerate() {
                lines.insert(insert_at + i, nl);
            }
            let new_content = lines.join("\n") + "\n";
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("editor insert: write error: {e}"))?;
            Ok(format!("Inserted at line {} in {}", insert_line, path.display()))
        }
        other => Err(format!("editor: unknown command '{other}'")),
    }
}

// ── grep_search ──────────────────────────────────────────────────────────────

fn tool_grep(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"].as_str()
        .ok_or("grep: missing 'pattern'")?;
    let search_path = input["path"].as_str()
        .ok_or("grep: missing 'path'")?;
    let include = input["include"].as_str().unwrap_or("");

    let resolved = resolve_path(search_path, workspace);

    let mut cmd = std::process::Command::new("grep");
    cmd.args(["-rn", "--color=never", "-E", pattern])
        .current_dir(workspace);
    if !include.is_empty() {
        cmd.arg("--include").arg(include);
    }
    cmd.arg(resolved.to_string_lossy().as_ref());

    let output = cmd.output()
        .map_err(|e| format!("grep: {e}"))?;

    let result = String::from_utf8_lossy(&output.stdout).to_string();
    if result.is_empty() {
        Ok("(no matches)".to_string())
    } else {
        // Limit output lines
        let limited: String = result.lines().take(200).collect::<Vec<_>>().join("\n");
        if result.lines().count() > 200 {
            Ok(format!("{limited}\n... [truncated, >200 matches]"))
        } else {
            Ok(limited)
        }
    }
}

// ── glob_find ────────────────────────────────────────────────────────────────

fn tool_glob(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"].as_str()
        .ok_or("glob: missing 'pattern'")?;
    let search_path = input["path"].as_str()
        .ok_or("glob: missing 'path'")?;

    let resolved = resolve_path(search_path, workspace);

    // Use find command as a portable glob implementation
    let output = std::process::Command::new("find")
        .arg(resolved.to_string_lossy().as_ref())
        .args(["-name", pattern, "-type", "f"])
        .arg("-maxdepth").arg("8")
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

// ── Helpers ──────────────────────────────────────────────────────────────────

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
        "bash" => input["command"].as_str().unwrap_or("").chars().take(150).collect(),
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
        _ => serde_json::to_string(input).unwrap_or_default().chars().take(150).collect(),
    }
}
