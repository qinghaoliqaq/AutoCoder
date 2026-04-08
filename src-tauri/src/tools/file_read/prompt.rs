/// Tool usage prompt — injected into the system prompt so the model
/// understands when and how to use this tool.
pub const PROMPT: &str = r#"
Reads a file from the local filesystem. You can access any file directly by using this tool.
Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- By default, it reads up to 2000 lines starting from the beginning of the file
- When you already know which part of the file you need, only read that part. This can be important for larger files.
- Results are returned using cat -n format, with line numbers starting at 1
- Binary files (images, PDFs, etc.) will be detected and a summary returned instead of raw bytes.
- This tool can only read files, not directories. To read a directory, use an ls command via the Bash tool.
- If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents.
"#;
