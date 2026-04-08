pub mod prompt;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolResult};

/// Hardcoded list of all known tool names for the stub implementation.
/// The real implementation would do fuzzy matching against deferred tools.
const ALL_TOOL_NAMES: &[&str] = &[
    "Agent",
    "AskUserQuestion",
    "Bash",
    "Brief",
    "Config",
    "Edit",
    "EnterPlanMode",
    "EnterWorktree",
    "ExitPlanMode",
    "ExitWorktree",
    "Glob",
    "Grep",
    "ListMcpResources",
    "LSP",
    "McpAuth",
    "MCP",
    "NotebookEdit",
    "PowerShell",
    "Read",
    "ReadMcpResource",
    "RemoteTrigger",
    "REPL",
    "ScheduleCron",
    "SendMessage",
    "Skill",
    "Sleep",
    "SyntheticOutput",
    "TaskCreate",
    "TaskGet",
    "TaskList",
    "TaskOutput",
    "TaskStop",
    "TaskUpdate",
    "TeamCreate",
    "TeamDelete",
    "TodoWrite",
    "ToolSearch",
    "WebFetch",
    "WebSearch",
    "Write",
];

/// ToolSearch tool — fetches full schema definitions for deferred tools so
/// they can be called.
///
/// Stub: returns a filtered list of hardcoded tool names matching the query.
/// The real implementation would do fuzzy matching against deferred tool
/// names and descriptions, and return full JSON Schema definitions.
pub struct ToolSearchTool;

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &'static str {
        "ToolSearch"
    }

    fn description(&self) -> &'static str {
        "Fetches full schema definitions for deferred tools so they can be called. \
         Deferred tools appear by name in <system-reminder> messages. Until fetched, \
         only the name is known — there is no parameter schema, so the tool cannot be \
         invoked. This tool takes a query, matches it against the deferred tool list, \
         and returns the matched tools' complete JSONSchema definitions. Query forms: \
         \"select:Read,Edit,Grep\" — fetch these exact tools by name; \
         \"notebook jupyter\" — keyword search, up to max_results best matches; \
         \"+slack send\" — require \"slack\" in the name, rank by remaining terms."
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5
                }
            },
            "additionalProperties": false
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        let query = match input["query"].as_str() {
            Some(q) if !q.is_empty() => q,
            _ => return ToolResult::err("Missing required parameter: query"),
        };

        let max_results = input["max_results"]
            .as_u64()
            .unwrap_or(5) as usize;

        // Handle "select:" prefix — direct tool selection
        if let Some(names_str) = query.strip_prefix("select:") {
            let requested: Vec<&str> = names_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let mut found = Vec::new();
            let mut missing = Vec::new();

            for name in &requested {
                if ALL_TOOL_NAMES.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                    found.push(*name);
                } else {
                    missing.push(*name);
                }
            }

            if found.is_empty() {
                return ToolResult::ok(format!(
                    "No matching deferred tools found for: {}",
                    missing.join(", ")
                ));
            }

            let result = format!(
                "Found {} tool(s): {}{}",
                found.len(),
                found.join(", "),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(" (not found: {})", missing.join(", "))
                }
            );
            return ToolResult::ok(result);
        }

        // Keyword search — simple case-insensitive substring matching
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .collect();

        let mut scored: Vec<(&str, usize)> = ALL_TOOL_NAMES
            .iter()
            .filter_map(|name| {
                let name_lower = name.to_lowercase();
                let mut score = 0usize;

                for term in &terms {
                    let term_clean = term.strip_prefix('+').unwrap_or(term);
                    if name_lower.contains(term_clean) {
                        score += 10;
                    }
                }

                if score > 0 {
                    Some((*name, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.truncate(max_results);

        if scored.is_empty() {
            // Return full tool list when no matches found
            let all_names: Vec<&str> = ALL_TOOL_NAMES
                .iter()
                .copied()
                .take(max_results)
                .collect();
            return ToolResult::ok(format!(
                "No matches for \"{query}\". Available tools ({} total): {}",
                ALL_TOOL_NAMES.len(),
                all_names.join(", ")
            ));
        }

        let matches: Vec<&str> = scored.iter().map(|(name, _)| *name).collect();
        ToolResult::ok(format!(
            "Found {} match(es) for \"{query}\": {}",
            matches.len(),
            matches.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn make_ctx(token: &CancellationToken) -> ToolContext<'_> {
        ToolContext {
            workspace: Path::new("/tmp"),
            read_only: false,
            token,
        }
    }

    #[tokio::test]
    async fn tool_search_select() {
        let tool = ToolSearchTool;
        assert_eq!(tool.name(), "ToolSearch");
        assert!(tool.is_read_only(&json!({})));

        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool
            .execute(json!({"query": "select:Read,Edit,Grep"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Read"));
        assert!(result.content.contains("Edit"));
        assert!(result.content.contains("Grep"));
    }

    #[tokio::test]
    async fn tool_search_keyword() {
        let tool = ToolSearchTool;
        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool
            .execute(json!({"query": "bash"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Bash"));
    }

    #[tokio::test]
    async fn tool_search_no_matches() {
        let tool = ToolSearchTool;
        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool
            .execute(json!({"query": "xyznonexistent"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("No matches"));
        assert!(result.content.contains("Available tools"));
    }

    #[tokio::test]
    async fn tool_search_with_max_results() {
        let tool = ToolSearchTool;
        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool
            .execute(json!({"query": "task", "max_results": 2}), &ctx)
            .await;
        assert!(!result.is_error);
        // Should find task-related tools but limited to 2
        assert!(result.content.contains("Task"));
    }

    #[tokio::test]
    async fn tool_search_missing_query() {
        let tool = ToolSearchTool;
        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.content.contains("query"));
    }

    #[tokio::test]
    async fn tool_search_select_missing() {
        let tool = ToolSearchTool;
        let token = CancellationToken::new();
        let ctx = make_ctx(&token);
        let result = tool
            .execute(json!({"query": "select:FakeToolXYZ"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("No matching"));
    }
}
