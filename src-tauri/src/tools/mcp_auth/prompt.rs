/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
The MCP server is installed but requires authentication. Call this tool to start the OAuth flow —
you'll receive an authorization URL to share with the user. Once the user completes authorization
in their browser, the server's real tools will become available automatically.

When called, this tool starts the OAuth flow with skipBrowserOpen and returns the authorization
URL. The OAuth callback completes in the background; once it fires, the server reconnects and
its real tools are swapped in automatically, replacing this pseudo-tool.
"#;
