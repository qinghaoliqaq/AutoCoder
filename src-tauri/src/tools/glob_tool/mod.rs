pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use glob::glob as glob_match;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct GlobTool;

/// Maximum number of file results returned by default.
const MAX_RESULTS: usize = 100;

const GLOB_DESCRIPTION: &str = r#"- Fast file pattern matching tool that works with any codebase size
- Supports glob patterns like "**/*.js" or "src/**/*.ts"
- Returns matching file paths sorted by modification time
- Use this tool when you need to find files by name patterns
- When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead"#;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "Glob"
    }

    fn description(&self) -> &'static str {
        GLOB_DESCRIPTION
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
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. DO NOT enter \"undefined\" or \"null\" - simply omit it for the default behavior. Must be a valid directory path if provided."
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

        // Resolve the search directory
        let search_dir = if let Some(path_str) = input["path"].as_str() {
            if path_str.is_empty() || path_str == "undefined" || path_str == "null" {
                ctx.workspace.to_path_buf()
            } else {
                match super::path_utils::resolve_path(path_str, ctx.workspace) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::err(format!("Invalid path: {e}")),
                }
            }
        } else {
            ctx.workspace.to_path_buf()
        };

        if !search_dir.exists() {
            return ToolResult::err(format!(
                "Directory does not exist: {}",
                search_dir.display()
            ));
        }

        if !search_dir.is_dir() {
            return ToolResult::err(format!("Path is not a directory: {}", search_dir.display()));
        }

        // Build the full glob pattern by combining the directory and the user pattern
        let full_pattern = if pattern.starts_with('/') || pattern.starts_with('\\') {
            // Absolute pattern: use as-is
            pattern.to_string()
        } else {
            // Relative pattern: prefix with search directory
            let dir_str = search_dir.to_string_lossy();
            let separator = if dir_str.ends_with('/') { "" } else { "/" };
            format!("{dir_str}{separator}{pattern}")
        };

        // Execute the glob pattern matching
        let entries = match glob_match(&full_pattern) {
            Ok(paths) => paths,
            Err(e) => {
                return ToolResult::err(format!("Invalid glob pattern: {e}"));
            }
        };

        // Collect all matching file paths with their modification times
        let mut matches: Vec<(PathBuf, u64)> = Vec::new();

        for entry in entries {
            match entry {
                Ok(path) => {
                    // Skip .git directories
                    let path_str = path.to_string_lossy();
                    if path_str.contains("/.git/") || path_str.ends_with("/.git") {
                        continue;
                    }

                    // Only include files, not directories
                    if path.is_file() {
                        let mtime = path
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .ok()
                                    .map(|d| d.as_millis() as u64)
                            })
                            .unwrap_or(0);
                        matches.push((path, mtime));
                    }
                }
                Err(_) => {
                    // Skip entries that can't be read (permission errors, etc.)
                    continue;
                }
            }
        }

        let total_matches = matches.len();

        // Sort by modification time (most recent first), with filename as tiebreaker
        matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Apply the result limit
        let truncated = matches.len() > MAX_RESULTS;
        matches.truncate(MAX_RESULTS);

        if matches.is_empty() {
            return ToolResult::ok("No files found");
        }

        // Format the result: file paths relative to workspace when possible
        let file_list: Vec<String> = matches
            .iter()
            .map(|(path, _)| {
                path.strip_prefix(ctx.workspace)
                    .map(|rel| rel.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string())
            })
            .collect();

        let mut result = file_list.join("\n");

        if truncated {
            result.push_str(&format!(
                "\n\n(Results truncated: showing {MAX_RESULTS} of {total_matches} total matches. \
                 Consider using a more specific path or pattern.)"
            ));
        }

        ToolResult::ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_always_read_only() {
        let tool = GlobTool;
        assert!(tool.is_read_only(&json!({"pattern": "**/*.rs"})));
    }

    #[test]
    fn test_name() {
        let tool = GlobTool;
        assert_eq!(tool.name(), "Glob");
    }

    #[test]
    fn test_input_schema_has_required_pattern() {
        let tool = GlobTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("pattern")));
    }

    #[test]
    fn test_description_not_empty() {
        let tool = GlobTool;
        assert!(!tool.description().is_empty());
    }
}
