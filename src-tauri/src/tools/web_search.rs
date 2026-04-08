use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &'static str {
        "WebSearch"
    }

    fn description(&self) -> &'static str {
        "Search the web for current information and return results.\n\
         \n\
         - Allows searching the web and using the results to inform responses\n\
         - Provides up-to-date information for current events and recent data\n\
         - Returns search result information including titles and URLs\n\
         - Use this tool for accessing information beyond the knowledge cutoff\n\
         \n\
         Usage notes:\n\
         - The query should be a clear, specific search string\n\
         - Domain filtering is supported via allowed_domains and blocked_domains\n\
         - Cannot specify both allowed_domains and blocked_domains in the same request\n\
         \n\
         CRITICAL REQUIREMENT:\n\
         - After answering the user's question, you MUST include a \"Sources:\" section\n\
         - In the Sources section, list all relevant URLs as markdown hyperlinks\n\
         - This is MANDATORY - never skip including sources in your response"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to use"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Only include search results from these domains"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Never include search results from these domains"
                }
            },
            "required": ["query"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext<'_>) -> ToolResult {
        // Validate input
        let query = match input.get("query").and_then(|v| v.as_str()) {
            Some(q) if !q.is_empty() => q,
            _ => return ToolResult::err("Missing or empty required parameter: query"),
        };

        // Check for conflicting domain filters
        let has_allowed = input
            .get("allowed_domains")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let has_blocked = input
            .get("blocked_domains")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);

        if has_allowed && has_blocked {
            return ToolResult::err(
                "Cannot specify both allowed_domains and blocked_domains in the same request",
            );
        }

        // Stub: web search not available in local mode
        let _ = query; // acknowledge usage
        ToolResult::err(
            "Web search is not available in local mode. \
             Use WebFetch with a known URL instead.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_web_search() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "WebSearch");
    }

    #[test]
    fn is_always_read_only() {
        let tool = WebSearchTool;
        assert!(tool.is_read_only(&json!({})));
    }

    #[test]
    fn schema_has_required_query() {
        let tool = WebSearchTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn schema_has_domain_filters() {
        let tool = WebSearchTool;
        let schema = tool.input_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("allowed_domains"));
        assert!(props.contains_key("blocked_domains"));
    }
}
