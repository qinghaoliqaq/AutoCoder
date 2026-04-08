pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

pub struct GrepTool;

/// Default cap on results when head_limit is not specified.
const DEFAULT_HEAD_LIMIT: usize = 250;

/// Version control directories to exclude from searches.
const VCS_DIRECTORIES: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];

const GREP_DESCRIPTION: &str = r#"A powerful search tool built on ripgrep

  Usage:
  - ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command. The Grep tool has been optimized for correct permissions and access.
  - Supports full regex syntax (e.g., "log.*Error", "function\s+\w+")
  - Filter files with glob parameter (e.g., "*.js", "**/*.tsx") or type parameter (e.g., "js", "py", "rust")
  - Output modes: "content" shows matching lines, "files_with_matches" shows only file paths (default), "count" shows match counts
  - Use Agent tool for open-ended searches requiring multiple rounds
  - Pattern syntax: Uses ripgrep (not grep) - literal braces need escaping (use `interface\{\}` to find `interface{}` in Go code)
  - Multiline matching: By default patterns match within single lines only. For cross-line patterns like `struct \{[\s\S]*?field`, use `multiline: true`
"#;

/// Check whether `rg` (ripgrep) is available on the system PATH.
async fn has_ripgrep() -> bool {
    Command::new("rg")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Apply offset and head_limit to a list of items. Returns the sliced items and
/// whether truncation occurred.
fn apply_head_limit<T: Clone>(
    items: &[T],
    head_limit: Option<usize>,
    offset: usize,
) -> (Vec<T>, bool) {
    // Explicit 0 means unlimited
    if head_limit == Some(0) {
        let sliced = items.iter().skip(offset).cloned().collect::<Vec<_>>();
        return (sliced, false);
    }
    let effective_limit = head_limit.unwrap_or(DEFAULT_HEAD_LIMIT);
    let remaining = if offset < items.len() {
        &items[offset..]
    } else {
        &[]
    };
    let truncated = remaining.len() > effective_limit;
    let sliced = remaining.iter().take(effective_limit).cloned().collect();
    (sliced, truncated)
}

/// Build the ripgrep argument list from the tool input.
fn build_rg_args(input: &Value, search_path: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // Include hidden files
    args.push("--hidden".to_string());

    // Exclude VCS directories
    for dir in VCS_DIRECTORIES {
        args.push("--glob".to_string());
        args.push(format!("!{dir}"));
    }

    // Limit line length to prevent base64/minified noise
    args.push("--max-columns".to_string());
    args.push("500".to_string());

    let output_mode = input["output_mode"]
        .as_str()
        .unwrap_or("files_with_matches");

    // Multiline
    let multiline = input["multiline"].as_bool().unwrap_or(false);
    if multiline {
        args.push("-U".to_string());
        args.push("--multiline-dotall".to_string());
    }

    // Case insensitive
    if input["-i"].as_bool().unwrap_or(false) {
        args.push("-i".to_string());
    }

    // Output mode flags
    match output_mode {
        "files_with_matches" => {
            args.push("-l".to_string());
        }
        "count" => {
            args.push("-c".to_string());
        }
        _ => {} // "content" uses default rg output
    }

    // Line numbers (only for content mode)
    if output_mode == "content" {
        let show_line_numbers = input["-n"].as_bool().unwrap_or(true);
        if show_line_numbers {
            args.push("-n".to_string());
        }
    }

    // Context flags (only for content mode)
    if output_mode == "content" {
        let context = input["context"].as_u64().or_else(|| input["-C"].as_u64());
        let context_before = input["-B"].as_u64();
        let context_after = input["-A"].as_u64();

        if let Some(c) = context {
            args.push("-C".to_string());
            args.push(c.to_string());
        } else {
            if let Some(b) = context_before {
                args.push("-B".to_string());
                args.push(b.to_string());
            }
            if let Some(a) = context_after {
                args.push("-A".to_string());
                args.push(a.to_string());
            }
        }
    }

    // Pattern
    let pattern = input["pattern"].as_str().unwrap_or("");
    if pattern.starts_with('-') {
        args.push("-e".to_string());
        args.push(pattern.to_string());
    } else {
        args.push(pattern.to_string());
    }

    // Type filter
    if let Some(file_type) = input["type"].as_str() {
        args.push("--type".to_string());
        args.push(file_type.to_string());
    }

    // Glob filter
    if let Some(glob_pattern) = input["glob"].as_str() {
        // Split on whitespace, but preserve patterns with braces
        for raw_pattern in glob_pattern.split_whitespace() {
            if raw_pattern.contains('{') && raw_pattern.contains('}') {
                args.push("--glob".to_string());
                args.push(raw_pattern.to_string());
            } else {
                // Split on commas for patterns without braces
                for sub_pattern in raw_pattern.split(',').filter(|s| !s.is_empty()) {
                    args.push("--glob".to_string());
                    args.push(sub_pattern.to_string());
                }
            }
        }
    }

    // Search path
    args.push(search_path.to_string());

    args
}

/// Build the GNU grep fallback argument list.
fn build_grep_args(input: &Value, search_path: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    args.push("-r".to_string());
    args.push("-n".to_string());

    // Exclude VCS directories
    for dir in VCS_DIRECTORIES {
        args.push(format!("--exclude-dir={dir}"));
    }

    let output_mode = input["output_mode"]
        .as_str()
        .unwrap_or("files_with_matches");

    // Case insensitive
    if input["-i"].as_bool().unwrap_or(false) {
        args.push("-i".to_string());
    }

    // Output mode flags
    match output_mode {
        "files_with_matches" => {
            args.push("-l".to_string());
        }
        "count" => {
            args.push("-c".to_string());
        }
        _ => {}
    }

    // Context flags (only for content mode with GNU grep)
    if output_mode == "content" {
        let context = input["context"].as_u64().or_else(|| input["-C"].as_u64());
        let context_before = input["-B"].as_u64();
        let context_after = input["-A"].as_u64();

        if let Some(c) = context {
            args.push("-C".to_string());
            args.push(c.to_string());
        } else {
            if let Some(b) = context_before {
                args.push("-B".to_string());
                args.push(b.to_string());
            }
            if let Some(a) = context_after {
                args.push("-A".to_string());
                args.push(a.to_string());
            }
        }
    }

    // Glob filter (GNU grep uses --include)
    if let Some(glob_pattern) = input["glob"].as_str() {
        for raw_pattern in glob_pattern.split_whitespace() {
            if raw_pattern.contains('{') && raw_pattern.contains('}') {
                args.push(format!("--include={raw_pattern}"));
            } else {
                for sub_pattern in raw_pattern.split(',').filter(|s| !s.is_empty()) {
                    args.push(format!("--include={sub_pattern}"));
                }
            }
        }
    }

    // Pattern (using -e for safety)
    let pattern = input["pattern"].as_str().unwrap_or("");
    args.push("-e".to_string());
    args.push(pattern.to_string());

    // Search path
    args.push(search_path.to_string());

    args
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "Grep"
    }

    fn description(&self) -> &'static str {
        GREP_DESCRIPTION
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (rg PATH). Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. \"*.js\", \"*.{ts,tsx}\") - maps to rg --glob"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: \"content\" shows matching lines (supports -A/-B/-C context, -n line numbers, head_limit), \"files_with_matches\" shows file paths (supports head_limit), \"count\" shows match counts (supports head_limit). Defaults to \"files_with_matches\"."
                },
                "-A": {
                    "type": "number",
                    "description": "Number of lines to show after each match (rg -A). Requires output_mode: \"content\", ignored otherwise."
                },
                "-B": {
                    "type": "number",
                    "description": "Number of lines to show before each match (rg -B). Requires output_mode: \"content\", ignored otherwise."
                },
                "-C": {
                    "type": "number",
                    "description": "Alias for context."
                },
                "context": {
                    "type": "number",
                    "description": "Number of lines to show before and after each match (rg -C). Requires output_mode: \"content\", ignored otherwise."
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers in output (rg -n). Requires output_mode: \"content\", ignored otherwise. Defaults to true."
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search (rg -i)"
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (rg --type). Common types: js, py, rust, go, java, etc. More efficient than include for standard file types."
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N lines/entries, equivalent to \"| head -N\". Works across all output modes: content (limits output lines), files_with_matches (limits file paths), count (limits count entries). Defaults to 250 when unspecified. Pass 0 for unlimited (use sparingly — large result sets waste context)."
                },
                "offset": {
                    "type": "number",
                    "description": "Skip first N lines/entries before applying head_limit, equivalent to \"| tail -n +N | head -N\". Works across all output modes. Defaults to 0."
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines and patterns can span lines (rg -U --multiline-dotall). Default: false."
                }
            }
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let pattern = match input["pattern"].as_str() {
            Some(p) if !p.is_empty() => p,
            _ => return ToolResult::err("Missing or empty 'pattern' parameter"),
        };

        // Resolve search path
        let search_path = if let Some(path_str) = input["path"].as_str() {
            match super::path_utils::resolve_path(path_str, ctx.workspace) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(format!("Invalid path: {e}")),
            }
        } else {
            ctx.workspace.to_path_buf()
        };

        let search_path_str = search_path.to_string_lossy().to_string();

        // Check if the path exists
        if !search_path.exists() {
            return ToolResult::err(format!("Path does not exist: {}", search_path.display()));
        }

        let output_mode = input["output_mode"]
            .as_str()
            .unwrap_or("files_with_matches");
        let head_limit = input["head_limit"].as_u64().map(|n| n as usize);
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;

        // Try ripgrep first, fall back to GNU grep
        let use_rg = has_ripgrep().await;

        let output = if use_rg {
            let args = build_rg_args(&input, &search_path_str);
            Command::new("rg").args(&args).output().await
        } else {
            let args = build_grep_args(&input, &search_path_str);
            Command::new("grep").args(&args).output().await
        };

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                return ToolResult::err(format!("Failed to execute search: {e}"));
            }
        };

        let exit_code = output.status.code().unwrap_or(-1);

        // rg/grep exit code 1 = no matches (not an error), 2+ = real error
        if exit_code >= 2 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return ToolResult::err(format!("Search failed (exit code {exit_code}): {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return match output_mode {
                "content" => ToolResult::ok("No matches found"),
                "count" => ToolResult::ok("No matches found"),
                _ => ToolResult::ok("No files found"),
            };
        }

        let lines: Vec<String> = stdout.lines().map(|l| l.to_string()).collect();

        match output_mode {
            "content" => {
                let (limited, truncated) = apply_head_limit(&lines, head_limit, offset);
                let mut result = limited.join("\n");
                if truncated || offset > 0 {
                    let mut pagination_parts = Vec::new();
                    if truncated {
                        pagination_parts.push(format!(
                            "limit: {}",
                            head_limit.unwrap_or(DEFAULT_HEAD_LIMIT)
                        ));
                    }
                    if offset > 0 {
                        pagination_parts.push(format!("offset: {offset}"));
                    }
                    result.push_str(&format!(
                        "\n\n[Showing results with pagination = {}]",
                        pagination_parts.join(", ")
                    ));
                }
                ToolResult::ok(result)
            }
            "count" => {
                let (limited, truncated) = apply_head_limit(&lines, head_limit, offset);

                // Parse count output to extract total matches and file count
                let mut total_matches: u64 = 0;
                let mut file_count: u64 = 0;
                for line in &limited {
                    if let Some(colon_pos) = line.rfind(':') {
                        if let Ok(count) = line[colon_pos + 1..].trim().parse::<u64>() {
                            total_matches += count;
                            file_count += 1;
                        }
                    }
                }

                let content = limited.join("\n");
                let occurrence_word = if total_matches == 1 {
                    "occurrence"
                } else {
                    "occurrences"
                };
                let file_word = if file_count == 1 { "file" } else { "files" };
                let mut result = format!(
                    "{content}\n\nFound {total_matches} total {occurrence_word} across {file_count} {file_word}."
                );
                if truncated || offset > 0 {
                    let mut pagination_parts = Vec::new();
                    if truncated {
                        pagination_parts.push(format!(
                            "limit: {}",
                            head_limit.unwrap_or(DEFAULT_HEAD_LIMIT)
                        ));
                    }
                    if offset > 0 {
                        pagination_parts.push(format!("offset: {offset}"));
                    }
                    result.push_str(&format!(
                        " with pagination = {}",
                        pagination_parts.join(", ")
                    ));
                }
                ToolResult::ok(result)
            }
            _ => {
                // files_with_matches mode
                let (limited, truncated) = apply_head_limit(&lines, head_limit, offset);
                let num_files = limited.len();

                let file_word = if num_files == 1 { "file" } else { "files" };
                let mut pagination_info = String::new();
                if truncated || offset > 0 {
                    let mut parts = Vec::new();
                    if truncated {
                        parts.push(format!(
                            "limit: {}",
                            head_limit.unwrap_or(DEFAULT_HEAD_LIMIT)
                        ));
                    }
                    if offset > 0 {
                        parts.push(format!("offset: {offset}"));
                    }
                    pagination_info = format!(" {}", parts.join(", "));
                }

                let result = format!(
                    "Found {num_files} {file_word}{pagination_info}\n{}",
                    limited.join("\n")
                );
                ToolResult::ok(result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_always_read_only() {
        let tool = GrepTool;
        assert!(tool.is_read_only(&json!({"pattern": "foo"})));
        assert!(tool.is_read_only(&json!({"pattern": "rm -rf"})));
    }

    #[test]
    fn test_apply_head_limit_default() {
        let items: Vec<i32> = (0..300).collect();
        let (result, truncated) = apply_head_limit(&items, None, 0);
        assert_eq!(result.len(), DEFAULT_HEAD_LIMIT);
        assert!(truncated);
    }

    #[test]
    fn test_apply_head_limit_explicit() {
        let items: Vec<i32> = (0..100).collect();
        let (result, truncated) = apply_head_limit(&items, Some(50), 0);
        assert_eq!(result.len(), 50);
        assert!(truncated);
    }

    #[test]
    fn test_apply_head_limit_zero_unlimited() {
        let items: Vec<i32> = (0..300).collect();
        let (result, truncated) = apply_head_limit(&items, Some(0), 0);
        assert_eq!(result.len(), 300);
        assert!(!truncated);
    }

    #[test]
    fn test_apply_head_limit_with_offset() {
        let items: Vec<i32> = (0..100).collect();
        let (result, truncated) = apply_head_limit(&items, Some(10), 50);
        assert_eq!(result.len(), 10);
        assert_eq!(result[0], 50);
        assert!(truncated);
    }

    #[test]
    fn test_name() {
        let tool = GrepTool;
        assert_eq!(tool.name(), "Grep");
    }

    #[test]
    fn test_build_rg_args_basic() {
        let input = json!({"pattern": "hello"});
        let args = build_rg_args(&input, "/tmp/workspace");
        assert!(args.contains(&"--hidden".to_string()));
        assert!(args.contains(&"hello".to_string()));
        assert!(args.contains(&"/tmp/workspace".to_string()));
        // Should have -l by default (files_with_matches)
        assert!(args.contains(&"-l".to_string()));
    }

    #[test]
    fn test_build_rg_args_content_mode() {
        let input = json!({
            "pattern": "hello",
            "output_mode": "content",
            "-n": true,
            "-A": 2,
            "-B": 1
        });
        let args = build_rg_args(&input, "/tmp");
        assert!(args.contains(&"-n".to_string()));
        assert!(args.contains(&"-A".to_string()));
        assert!(args.contains(&"-B".to_string()));
        assert!(!args.contains(&"-l".to_string()));
    }

    #[test]
    fn test_build_rg_args_dash_pattern() {
        let input = json!({"pattern": "--foo"});
        let args = build_rg_args(&input, "/tmp");
        // Should use -e flag for patterns starting with dash
        let e_pos = args.iter().position(|a| a == "-e").unwrap();
        assert_eq!(args[e_pos + 1], "--foo");
    }
}
