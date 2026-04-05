/// Agent SDK sidecar — manages the Node.js child process that bridges to
/// the Claude Agent SDK.
///
/// Communication: line-delimited JSON over stdin (requests) / stdout (responses).
///
/// The sidecar is started lazily on the first skill invocation that needs it
/// and kept alive for the duration of the app.

use crate::config::AgentConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, EventTarget};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::skills::{SkillChunk, ToolLog};

// ── Types ────────────────────────────────────────────────────────────────────

/// A response line from the sidecar.
#[derive(Debug, Deserialize)]
struct SidecarResponse {
    id:      String,
    #[serde(rename = "type")]
    kind:    String,
    #[serde(default)]
    text:    Option<String>,
    #[serde(default)]
    agent:   Option<String>,
    #[serde(default)]
    tool:    Option<String>,
    #[serde(default)]
    input:   Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    ok:      Option<bool>,
}

/// Handle to the running sidecar process.
pub struct SidecarHandle {
    stdin:  tokio::process::ChildStdin,
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    #[allow(dead_code)]
    child:  Child,
}

/// Shared sidecar state — lazily initialized, app-lifetime singleton.
pub struct SidecarState {
    handle: Mutex<Option<SidecarHandle>>,
}

impl SidecarState {
    pub fn new() -> Self {
        Self { handle: Mutex::new(None) }
    }
}

// ── Sidecar lifecycle ────────────────────────────────────────────────────────

/// Locate the sidecar entry point relative to the binary or project root.
fn find_sidecar_script() -> Result<PathBuf, String> {
    let candidates = [
        // Development: project root
        std::env::current_dir().ok().map(|d| d.join("agent-sidecar").join("index.mjs")),
        // Production: next to binary
        std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("agent-sidecar").join("index.mjs"))),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Cannot find agent-sidecar/index.mjs".to_string())
}

/// Start the sidecar if not already running, and return a guard for sending requests.
async fn ensure_running(
    state:  &SidecarState,
    config: &AgentConfig,
) -> Result<(), String> {
    let mut guard = state.handle.lock().await;
    if guard.is_some() {
        return Ok(());
    }

    let script = find_sidecar_script()?;

    let mut cmd = Command::new("node");
    cmd.arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Pass agent config as env vars so the Agent SDK picks them up
    if !config.api_key.is_empty() {
        cmd.env("ANTHROPIC_API_KEY", &config.api_key);
    }
    if !config.base_url.is_empty() {
        cmd.env("ANTHROPIC_BASE_URL", &config.base_url);
    }
    match config.provider.as_str() {
        "bedrock" => { cmd.env("CLAUDE_CODE_USE_BEDROCK", "1"); }
        "vertex"  => { cmd.env("CLAUDE_CODE_USE_VERTEX", "1"); }
        "foundry" => { cmd.env("CLAUDE_CODE_USE_FOUNDRY", "1"); }
        _ => {}
    }

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start agent sidecar: {e}"))?;

    let stdin = child.stdin.take()
        .ok_or("No stdin from sidecar")?;
    let stdout = child.stdout.take()
        .ok_or("No stdout from sidecar")?;

    // Drain stderr in the background
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        });
    }

    let reader = Arc::new(Mutex::new(BufReader::new(stdout)));

    // Wait for the "sidecar ready" init message
    {
        let mut r = reader.lock().await;
        let mut init_line = String::new();
        r.read_line(&mut init_line).await
            .map_err(|e| format!("Sidecar init read error: {e}"))?;
        let v: Value = serde_json::from_str(init_line.trim())
            .map_err(|e| format!("Sidecar init parse error: {e}"))?;
        if v["type"] != "result" || v["ok"] != true {
            return Err(format!("Sidecar init failed: {}", init_line.trim()));
        }
    }

    *guard = Some(SidecarHandle { stdin, stdout: reader, child });
    Ok(())
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Run an Agent SDK query through the sidecar, streaming chunks and tool logs
/// to the frontend via Tauri events. Returns the full accumulated text.
pub async fn run_agent_query(
    state:        &SidecarState,
    config:       &AgentConfig,
    prompt:       &str,
    cwd:          Option<&str>,
    permission:   &str,           // "acceptEdits" | "plan" | etc.
    allowed_tools: &[&str],
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<String, String> {
    ensure_running(state, config).await?;

    let req_id = format!("req-{}", chrono::Utc::now().timestamp_millis());

    let request = json!({
        "id":     req_id,
        "action": "query",
        "prompt": prompt,
        "options": {
            "cwd":            cwd.unwrap_or("."),
            "allowedTools":   allowed_tools,
            "permissionMode": permission,
            "model":          if config.model.is_empty() { None } else { Some(&config.model) },
        }
    });

    let mut guard = state.handle.lock().await;
    let handle = guard.as_mut().ok_or("Sidecar not running")?;

    // Send request
    let line = serde_json::to_string(&request)
        .map_err(|e| format!("JSON serialize error: {e}"))?;
    handle.stdin.write_all(format!("{line}\n").as_bytes()).await
        .map_err(|e| format!("Sidecar write error: {e}"))?;
    handle.stdin.flush().await
        .map_err(|e| format!("Sidecar flush error: {e}"))?;

    let reader = handle.stdout.clone();
    drop(guard); // Release lock during streaming

    // Read response lines until we get a "result" or "error" for our request
    let mut full_text = String::new();
    let mut is_first_chunk = true;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(1800));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                // Send cancel request
                let cancel_req = json!({
                    "id": format!("cancel-{req_id}"),
                    "action": "cancel",
                    "targetId": req_id
                });
                if let Ok(mut g) = state.handle.try_lock() {
                    if let Some(h) = g.as_mut() {
                        let cancel_line = serde_json::to_string(&cancel_req).unwrap_or_default();
                        let _ = h.stdin.write_all(format!("{cancel_line}\n").as_bytes()).await;
                        let _ = h.stdin.flush().await;
                    }
                }
                return Err("cancelled".to_string());
            }
            _ = &mut timeout => {
                return Err("Agent query timed out after 1800s".to_string());
            }
            line = async {
                let mut r = reader.lock().await;
                let mut buf = String::new();
                r.read_line(&mut buf).await.map(|_| buf)
            } => {
                let line = line.map_err(|e| format!("Sidecar read error: {e}"))?;
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                let resp: SidecarResponse = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(_) => continue, // Skip malformed lines
                };

                // Only process responses for our request
                if resp.id != req_id { continue; }

                match resp.kind.as_str() {
                    "chunk" => {
                        if let Some(text) = &resp.text {
                            full_text.push_str(text);
                            let reset = is_first_chunk;
                            is_first_chunk = false;
                            let _ = app_handle.emit_to(
                                EventTarget::webview_window(window_label),
                                "skill-chunk",
                                SkillChunk {
                                    agent: resp.agent.unwrap_or_else(|| "claude".to_string()),
                                    text:  text.clone(),
                                    reset,
                                },
                            );
                        }
                    }
                    "tool" => {
                        let ts = chrono::Utc::now().timestamp_millis() as u64;
                        let _ = app_handle.emit_to(
                            EventTarget::webview_window(window_label),
                            "tool-log",
                            ToolLog {
                                agent:     "claude".to_string(),
                                tool:      resp.tool.unwrap_or_default(),
                                input:     resp.input.unwrap_or_default(),
                                timestamp: ts,
                            },
                        );
                    }
                    "result" => {
                        if let Some(text) = &resp.text {
                            if full_text.is_empty() {
                                full_text = text.clone();
                            }
                        }
                        return Ok(full_text);
                    }
                    "error" => {
                        return Err(format!("Agent error: {}", resp.message.unwrap_or_default()));
                    }
                    _ => {}
                }
            }
        }
    }
}
