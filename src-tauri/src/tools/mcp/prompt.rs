/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
Execute a tool provided by an MCP (Model Context Protocol) server. MCP servers expose additional
tools that extend the assistant's capabilities. The actual prompt and description for each MCP
tool are provided dynamically by the connected MCP server.

Use this tool to invoke a specific tool on a named MCP server, passing any arguments the server
tool requires. The server_name and tool_name identify which MCP tool to call, and arguments
contains the input the server tool expects.
"#;
