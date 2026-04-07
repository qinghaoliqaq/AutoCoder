/// Local tool execution — bash, editor, grep, glob.
///
/// All execution happens in-process (Rust), no external CLI needed.
/// Includes partitioned orchestration: read-only tools run concurrently,
/// write tools run serially.

use super::tools;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

const MAX_TOOL_CONCURRENCY: usize = 10;
const LARGE_RESULT_THRESHOLD: usize = 30_000;
const LARGE_RESULT_PREVIEW: usize = 2_000;
const MAX_RESULT_CHARS: usize = 50_000;

// ── Partitioned orchestration ───────────────────────────────────────────────

/// Execute tool calls with read-only batching (concurrent) and write (serial).
/// When `read_only` is true, bash and editor write commands are rejected at runtime.
pub async fn run_partitioned(
    tool_calls: &[(String, String, Value)],
    workspace: &Path,
    token: &CancellationToken,
    read_only: bool,
) -> Result<Vec<Value>, String> {
    let mut batches: Vec<(bool, Vec<usize>)> = Vec::new();
    for (i, (_id, name, input)) in tool_calls.iter().enumerate() {
        let readonly = tools::is_read_only(name, input);
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
            run_concurrent_batch(tool_calls, indices, workspace, read_only, &mut results).await?;
        } else {
            run_serial_batch(
                tool_calls,
                indices,
                workspace,
                token,
                read_only,
                &mut results,
            )
            .await?;
        }
    }

    Ok(results)
}

async fn run_concurrent_batch(
    tool_calls: &[(String, String, Value)],
    indices: &[usize],
    workspace: &Path,
    read_only: bool,
    results: &mut [Value],
) -> Result<(), String> {
    let mut handles = Vec::new();
    for &idx in indices {
        let (id, name, input) = &tool_calls[idx];
        let id = id.clone();
        let name = name.clone();
        let input = input.clone();
        let ws = workspace.to_path_buf();
        handles.push(tokio::spawn(async move {
            let result = dispatch(&name, &input, &ws, read_only).await;
            (idx, id, name, result)
        }));
    }

    for chunk in handles.chunks_mut(MAX_TOOL_CONCURRENCY) {
        let chunk_results: Vec<_> =
            futures::future::join_all(chunk.iter_mut().map(|h| async { h.await })).await;

        for join_result in chunk_results {
            let (idx, id, name, result) =
                join_result.map_err(|e| format!("Tool task join error: {e}"))?;
            results[idx] = build_tool_result(&id, &name, result);
        }
    }
    Ok(())
}

async fn run_serial_batch(
    tool_calls: &[(String, String, Value)],
    indices: &[usize],
    workspace: &Path,
    token: &CancellationToken,
    read_only: bool,
    results: &mut [Value],
) -> Result<(), String> {
    for &idx in indices {
        if token.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let (id, name, input) = &tool_calls[idx];
        let result = dispatch(name, input, workspace, read_only).await;
        results[idx] = build_tool_result(id, name, result);
    }
    Ok(())
}

fn build_tool_result(id: &str, tool_name: &str, result: Result<String, String>) -> Value {
    let (output, is_error) = match result {
        Ok(out) => (out, false),
        Err(err) => (err, true),
    };
    let processed = maybe_persist_large_result(&output, tool_name);
    let mut obj = json!({
        "type": "tool_result",
        "tool_use_id": id,
        "content": processed,
    });
    if is_error {
        obj["is_error"] = json!(true);
    }
    obj
}

// ── Tool dispatch ───────────────────────────────────────────────────────────

async fn dispatch(
    name: &str,
    input: &Value,
    workspace: &Path,
    read_only: bool,
) -> Result<String, String> {
    // Runtime enforcement: reject write tools in read-only mode.
    // This is a defense-in-depth layer — even if schema filtering fails to
    // exclude a tool, the dispatch layer blocks it.
    if read_only {
        match name {
            "bash" => return Err("bash: blocked in read-only mode".to_string()),
            "str_replace_based_edit_tool" => {
                let cmd = input["command"].as_str().unwrap_or("");
                if cmd != "view" {
                    return Err(format!(
                        "editor: '{cmd}' blocked in read-only mode (only 'view' allowed)"
                    ));
                }
            }
            _ => {} // grep, glob are always safe
        }
    }

    match name {
        "bash" => tool_bash(input, workspace).await,
        "str_replace_based_edit_tool" => tool_editor(input, workspace),
        "grep_search" => tool_grep(input, workspace),
        "glob_find" => tool_glob(input, workspace),
        other => Err(format!("Unknown tool: {other}")),
    }
}

// ── bash ────────────────────────────────────────────────────────────────────

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

// ── text editor ─────────────────────────────────────────────────────────────

fn tool_editor(input: &Value, workspace: &Path) -> Result<String, String> {
    let command = input["command"]
        .as_str()
        .ok_or("editor: missing 'command' field")?;
    let path_str = input["path"]
        .as_str()
        .ok_or("editor: missing 'path' field")?;
    let path = resolve_path(path_str, workspace)?;

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
            Ok(format!("Inserted at line {} in {}", insert_line, path.display()))
        }
        other => Err(format!("editor: unknown command '{other}'")),
    }
}

// ── grep ────────────────────────────────────────────────────────────────────

fn tool_grep(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .ok_or("grep: missing 'pattern'")?;
    let search_path = input["path"].as_str().ok_or("grep: missing 'path'")?;
    let include = input["include"].as_str().unwrap_or("");
    let resolved = resolve_path(search_path, workspace)?;

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

// ── glob ────────────────────────────────────────────────────────────────────

fn tool_glob(input: &Value, workspace: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .ok_or("glob: missing 'pattern'")?;
    let search_path = input["path"].as_str().ok_or("glob: missing 'path'")?;
    let resolved = resolve_path(search_path, workspace)?;

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

fn resolve_path(path_str: &str, workspace: &Path) -> Result<PathBuf, String> {
    let p = Path::new(path_str);
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace.join(p)
    };
    // Canonicalize what exists; for new files, canonicalize the parent.
    let canonical = if resolved.exists() {
        resolved.canonicalize().map_err(|e| format!("path error: {e}"))?
    } else if let Some(parent) = resolved.parent() {
        let canon_parent = if parent.exists() {
            parent.canonicalize().map_err(|e| format!("path error: {e}"))?
        } else {
            parent.to_path_buf()
        };
        canon_parent.join(resolved.file_name().unwrap_or_default())
    } else {
        resolved.clone()
    };
    let ws_canonical = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    if !canonical.starts_with(&ws_canonical) {
        return Err(format!(
            "path '{}' escapes workspace boundary '{}'",
            path_str,
            ws_canonical.display()
        ));
    }
    Ok(canonical)
}

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
    let path = cache_dir.join(format!("{tool_name}_{ts}.txt"));
    if std::fs::write(&path, result).is_ok() {
        let preview_end = result
            .char_indices()
            .nth(LARGE_RESULT_PREVIEW)
            .map(|(i, _)| i)
            .unwrap_or(result.len());
        format!(
            "{}\n\n... [result too large: {} chars, saved to {}]",
            &result[..preview_end],
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
