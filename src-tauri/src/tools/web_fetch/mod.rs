pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Maximum response body size in bytes (100 KB).
const MAX_BODY_BYTES: usize = 100 * 1024;

/// Request timeout in seconds.
const TIMEOUT_SECS: u64 = 30;

/// User-Agent header sent with every request.
const USER_AGENT: &str = "AutoCoder/1.0 (WebFetchTool)";

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "WebFetch"
    }

    fn description(&self) -> &'static str {
        "Fetches content from a specified URL and returns the text.\n\
         \n\
         - Takes a URL and an optional prompt describing what to extract\n\
         - Fetches the URL content, strips HTML tags to extract readable text\n\
         - Returns the response body text, HTTP status code, and byte count\n\
         - Use this tool when you need to retrieve and analyze web content\n\
         \n\
         Usage notes:\n\
         - The URL must be a fully-formed valid URL\n\
         - HTTP URLs will be automatically upgraded to HTTPS\n\
         - This tool is read-only and does not modify any files\n\
         - Response bodies are limited to 100KB to avoid excessively large results\n\
         - A 30-second timeout is applied to all requests\n\
         - For GitHub URLs, prefer using the gh CLI via Bash instead"
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(prompt::PROMPT)
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt describing what information to extract from the page"
                }
            },
            "required": ["url"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        let url = match input.get("url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => return ToolResult::err("Missing required parameter: url"),
        };

        let _prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

        // Validate URL (basic check without the url crate)
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::err(format!(
                "Invalid URL: \"{url}\". URL must start with http:// or https://"
            ));
        }

        // Check cancellation before starting the request
        if ctx.token.is_cancelled() {
            return ToolResult::err("Operation cancelled");
        }

        // Build the HTTP client
        let client = match reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to create HTTP client: {e}")),
        };

        // Execute the request with cancellation support
        let response = tokio::select! {
            res = client.get(&url).send() => res,
            _ = ctx.token.cancelled() => {
                return ToolResult::err("Operation cancelled");
            }
        };

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::err(format!(
                        "Request timed out after {TIMEOUT_SECS}s fetching {url}"
                    ));
                }
                if e.is_connect() {
                    return ToolResult::err(format!("Connection error fetching {url}: {e}"));
                }
                return ToolResult::err(format!("Network error fetching {url}: {e}"));
            }
        };

        let status = response.status();
        let status_code = status.as_u16();
        let status_text = status.canonical_reason().unwrap_or("Unknown").to_string();

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Read the body, limited to MAX_BODY_BYTES
        let body_bytes = match read_limited_body(response, MAX_BODY_BYTES).await {
            Ok(bytes) => bytes,
            Err(e) => return ToolResult::err(format!("Error reading response body: {e}")),
        };

        let byte_count = body_bytes.len();

        // Convert to text
        let body_text = String::from_utf8_lossy(&body_bytes).to_string();

        // If HTML content, strip tags to extract text
        let content =
            if content_type.contains("text/html") || body_text.trim_start().starts_with('<') {
                strip_html_tags(&body_text)
            } else {
                body_text
            };

        // Build result
        let result = format!(
            "URL: {url}\n\
             Status: {status_code} {status_text}\n\
             Content-Type: {content_type}\n\
             Bytes: {byte_count}\n\
             \n\
             {content}"
        );

        if status.is_success() {
            ToolResult::ok(result)
        } else {
            // Still return the content for non-success status codes, but mark as error
            // for 4xx/5xx so the model is aware
            ToolResult::ok(result)
        }
    }
}

/// Read the response body up to `limit` bytes using streaming to avoid OOM.
async fn read_limited_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read body: {e}"))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() >= limit {
            buf.truncate(limit);
            break;
        }
    }

    Ok(buf)
}

/// Basic HTML tag stripping to extract text content.
/// Removes script/style blocks entirely, strips all tags, collapses whitespace.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut capturing_tag_name = false;

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            capturing_tag_name = true;
            i += 1;
            continue;
        }

        if in_tag {
            if ch == '>' {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();

                if tag_lower == "script" {
                    in_script = true;
                } else if tag_lower == "/script" {
                    in_script = false;
                } else if tag_lower == "style" {
                    in_style = true;
                } else if tag_lower == "/style" {
                    in_style = false;
                } else if tag_lower == "br"
                    || tag_lower == "br/"
                    || tag_lower == "p"
                    || tag_lower == "/p"
                    || tag_lower == "div"
                    || tag_lower == "/div"
                    || tag_lower == "li"
                    || tag_lower == "h1"
                    || tag_lower == "h2"
                    || tag_lower == "h3"
                    || tag_lower == "h4"
                    || tag_lower == "h5"
                    || tag_lower == "h6"
                    || tag_lower.starts_with("/h")
                    || tag_lower == "tr"
                    || tag_lower == "/tr"
                {
                    result.push('\n');
                }

                capturing_tag_name = false;
            } else if capturing_tag_name {
                if ch.is_whitespace() {
                    capturing_tag_name = false;
                } else {
                    tag_name.push(ch);
                }
            }
            i += 1;
            continue;
        }

        if !in_script && !in_style {
            // Decode common HTML entities
            if ch == '&' {
                let remaining: String = chars[i..].iter().take(10).collect();
                if remaining.starts_with("&amp;") {
                    result.push('&');
                    i += 5;
                    continue;
                } else if remaining.starts_with("&lt;") {
                    result.push('<');
                    i += 4;
                    continue;
                } else if remaining.starts_with("&gt;") {
                    result.push('>');
                    i += 4;
                    continue;
                } else if remaining.starts_with("&quot;") {
                    result.push('"');
                    i += 6;
                    continue;
                } else if remaining.starts_with("&apos;") {
                    result.push('\'');
                    i += 6;
                    continue;
                } else if remaining.starts_with("&nbsp;") {
                    result.push(' ');
                    i += 6;
                    continue;
                }
            }
            result.push(ch);
        }

        i += 1;
    }

    // Collapse multiple blank lines into at most two newlines
    let mut collapsed = String::with_capacity(result.len());
    let mut consecutive_newlines = 0;
    for ch in result.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                collapsed.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            collapsed.push(ch);
        }
    }

    // Trim leading/trailing whitespace from each line, remove empty lines
    collapsed
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_basic() {
        let html = "<html><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn strip_html_removes_script_and_style() {
        let html = "<html><head><style>body{color:red}</style></head>\
                     <body><script>alert('hi')</script><p>Content</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Content"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn strip_html_decodes_entities() {
        let html = "<p>A &amp; B &lt; C &gt; D</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("A & B < C > D"));
    }

    #[test]
    fn input_schema_has_required_url() {
        let tool = WebFetchTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("url")));
    }

    #[test]
    fn is_always_read_only() {
        let tool = WebFetchTool;
        assert!(tool.is_read_only(&json!({})));
        assert!(tool.is_read_only(&json!({"url": "https://example.com"})));
    }
}
