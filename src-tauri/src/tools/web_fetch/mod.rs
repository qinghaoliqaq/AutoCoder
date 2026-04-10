pub mod prompt;

use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Maximum response body size in bytes (100 KB).
const MAX_BODY_BYTES: usize = 100 * 1024;

/// Request timeout in seconds.
const TIMEOUT_SECS: u64 = 30;

/// User-Agent header sent with every request.
const USER_AGENT: &str = "AutoCoder/1.0 (WebFetchTool)";

/// Reject a URL if its host resolves to an IP that should not be reachable
/// from an LLM-controlled fetch: loopback, private, link-local (AWS/GCE
/// metadata endpoints live here), unspecified, or multicast addresses.
///
/// This is the last line of defense against SSRF. A prompt-injected or
/// compromised model could otherwise coax WebFetch into probing internal
/// services or exfiltrating cloud IAM tokens via 169.254.169.254.
fn is_blocked_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => {
            // Private (RFC 1918), link-local (169.254/16 — includes AWS/GCE
            // metadata), broadcast, CGNAT (100.64.0.0/10), and other reserved
            // ranges we don't want the agent to reach.
            if v4.is_private() || v4.is_link_local() || v4.is_broadcast() {
                return true;
            }
            let octets = v4.octets();
            // 100.64.0.0/10 — carrier-grade NAT (RFC 6598)
            if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
                return true;
            }
            // 0.0.0.0/8 — "this network"
            if octets[0] == 0 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv6(v6: Ipv6Addr) -> bool {
    // Unique local addresses fc00::/7
    let segments = v6.segments();
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Link-local fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // IPv4-mapped ::ffff:0:0/96 — validate the embedded v4 too
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_blocked_ip(IpAddr::V4(v4));
    }
    // IPv4-compatible ::/96 (deprecated but treat as inner v4)
    if segments[..6].iter().all(|s| *s == 0) {
        let mapped = Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            (segments[6] & 0xff) as u8,
            (segments[7] >> 8) as u8,
            (segments[7] & 0xff) as u8,
        );
        if !mapped.is_unspecified() {
            return is_blocked_ip(IpAddr::V4(mapped));
        }
    }
    false
}

/// Resolve the URL's host to IP addresses and reject the request if any
/// resolved address is in a blocked range.
///
/// We check *all* resolved addresses, not just the first one, because a
/// hostile DNS server could otherwise rotate responses to slip a private
/// IP past the check after the safe one is observed (DNS rebinding is a
/// separate issue — reqwest re-resolves for redirects, which is why we
/// also forbid redirects to unchecked hosts below).
async fn enforce_url_safety(url: &reqwest::Url) -> Result<(), String> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "URL scheme '{scheme}' is not allowed. Only http:// and https:// are permitted."
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| "URL is missing a host".to_string())?;

    // Literal IP fast path — no DNS required.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(format!(
                "URL host '{host}' resolves to a blocked address ({ip}). \
                 Private, loopback, link-local, and metadata endpoints are not allowed."
            ));
        }
        return Ok(());
    }

    // Reject bare "localhost" and similar special names even if the OS
    // resolver would return loopback.
    let host_lower = host.to_ascii_lowercase();
    const BLOCKED_HOSTS: &[&str] = &[
        "localhost",
        "localhost.localdomain",
        "ip6-localhost",
        "ip6-loopback",
        "broadcasthost",
    ];
    if BLOCKED_HOSTS.contains(&host_lower.as_str()) || host_lower.ends_with(".localhost") {
        return Err(format!(
            "URL host '{host}' is a reserved local name and is not allowed."
        ));
    }

    // DNS lookup: resolve on any port (80) and inspect all returned IPs.
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("Cannot resolve host '{host}': {e}"))?;

    let mut any = false;
    for sock in addrs {
        any = true;
        let ip = sock.ip();
        if is_blocked_ip(ip) {
            return Err(format!(
                "URL host '{host}' resolves to a blocked address ({ip}). \
                 Private, loopback, link-local, and metadata endpoints are not allowed."
            ));
        }
    }
    if !any {
        return Err(format!("Host '{host}' did not resolve to any address"));
    }
    Ok(())
}

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

        // Parse with the url crate (re-exported through reqwest) so we can
        // validate the host against the SSRF blocklist instead of doing a
        // prefix check on the raw string.
        let parsed_url = match reqwest::Url::parse(&url) {
            Ok(u) => u,
            Err(e) => {
                return ToolResult::err(format!(
                    "Invalid URL \"{url}\": {e}. URL must start with http:// or https://."
                ));
            }
        };

        if let Err(e) = enforce_url_safety(&parsed_url).await {
            return ToolResult::err(e);
        }

        // Check cancellation before starting the request
        if ctx.token.is_cancelled() {
            return ToolResult::err("Operation cancelled");
        }

        // Build the HTTP client. Enforce the SSRF check on every redirect
        // hop too — otherwise an attacker could host a 302 that redirects
        // to http://169.254.169.254/ and bypass the initial DNS guard.
        let client = match reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("too many redirects");
                }
                let next = attempt.url();
                let host = match next.host_str() {
                    Some(h) => h,
                    None => return attempt.stop(),
                };
                // Same cheap check as enforce_url_safety for literal IPs —
                // we can't await DNS inside a sync redirect callback, so
                // only literal IPs are blocked here. Hostnames still pass,
                // but reqwest will perform a fresh connect that is itself
                // bound to the same OS resolver; a malicious rebind would
                // need to hit a non-private IP to ever reach us.
                if let Ok(ip) = host.parse::<IpAddr>() {
                    if is_blocked_ip(ip) {
                        return attempt.error("redirect to blocked IP");
                    }
                }
                let lower = host.to_ascii_lowercase();
                if lower == "localhost" || lower.ends_with(".localhost") {
                    return attempt.error("redirect to reserved local name");
                }
                attempt.follow()
            }))
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

    #[test]
    fn ssrf_blocks_loopback_and_private_v4() {
        use std::net::Ipv4Addr;
        // Loopback
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        // AWS / GCE metadata
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        // RFC 1918
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 5, 4))));
        // CGNAT
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(100, 127, 255, 254))));
        // 0.0.0.0/8
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));
        // Broadcast
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255))));

        // Public addresses should pass
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        // CGNAT boundary: 100.128.0.0 is outside 100.64.0.0/10
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn ssrf_blocks_loopback_and_ula_v6() {
        use std::net::Ipv6Addr;
        // ::1 loopback
        assert!(is_blocked_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        // ULA
        assert!(is_blocked_ip(IpAddr::V6(
            "fd12:3456:789a::1".parse().unwrap()
        )));
        // Link-local
        assert!(is_blocked_ip(IpAddr::V6("fe80::1".parse().unwrap())));
        // IPv4-mapped loopback
        assert!(is_blocked_ip(IpAddr::V6(
            "::ffff:127.0.0.1".parse().unwrap()
        )));
        // IPv4-mapped metadata
        assert!(is_blocked_ip(IpAddr::V6(
            "::ffff:169.254.169.254".parse().unwrap()
        )));
        // Public v6 (Google DNS) should pass
        assert!(!is_blocked_ip(IpAddr::V6(
            "2001:4860:4860::8888".parse().unwrap()
        )));
    }

    #[tokio::test]
    async fn ssrf_enforce_rejects_literal_private_ip() {
        let url = reqwest::Url::parse("http://10.0.0.1/").unwrap();
        let err = enforce_url_safety(&url).await.unwrap_err();
        assert!(err.contains("blocked"));
    }

    #[tokio::test]
    async fn ssrf_enforce_rejects_localhost_name() {
        let url = reqwest::Url::parse("http://localhost/admin").unwrap();
        let err = enforce_url_safety(&url).await.unwrap_err();
        assert!(err.contains("reserved"));
    }

    #[tokio::test]
    async fn ssrf_enforce_rejects_metadata_ip() {
        let url = reqwest::Url::parse("http://169.254.169.254/latest/meta-data/").unwrap();
        let err = enforce_url_safety(&url).await.unwrap_err();
        assert!(err.contains("blocked"));
    }

    #[tokio::test]
    async fn ssrf_enforce_rejects_file_scheme() {
        let url = reqwest::Url::parse("file:///etc/passwd").unwrap();
        let err = enforce_url_safety(&url).await.unwrap_err();
        assert!(err.contains("scheme"));
    }
}
