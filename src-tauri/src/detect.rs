use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub claude: ToolInfo,
    pub codex: ToolInfo,
}

fn detect_tool(name: &str, version_args: &[&str]) -> ToolInfo {
    // First try to get the path
    let path = get_tool_path(name);

    // Then try to get the version
    let version_result = Command::new(name)
        .args(version_args)
        .output();

    match version_result {
        Ok(output) if output.status.success() => {
            let version_output = String::from_utf8_lossy(&output.stdout).to_string();
            let version = extract_version(&version_output)
                .or_else(|| {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    extract_version(&stderr)
                });

            ToolInfo {
                installed: true,
                version,
                path,
            }
        }
        Ok(output) => {
            // Command ran but returned non-zero — might still be installed
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let version = extract_version(&stdout).or_else(|| extract_version(&stderr));

            ToolInfo {
                installed: path.is_some(),
                version,
                path,
            }
        }
        Err(_) => ToolInfo {
            installed: false,
            version: None,
            path: None,
        },
    }
}

fn get_tool_path(name: &str) -> Option<String> {
    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    let output = Command::new(which_cmd)
        .arg(name)
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout)
            .trim()
            .split('\n')
            .next()
            .unwrap_or("")
            .to_string();

        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

fn extract_version(text: &str) -> Option<String> {
    // Match patterns like: "1.0.0", "v1.0.0", "version 1.0.0", "Claude 1.0.0"
    for line in text.lines().take(3) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Find version pattern: digits.digits(.digits)*
        let words: Vec<&str> = line.split_whitespace().collect();
        for word in &words {
            let cleaned = word.trim_start_matches('v');
            let parts: Vec<&str> = cleaned.split('.').collect();
            if parts.len() >= 2 && parts[0].chars().all(|c| c.is_ascii_digit()) {
                return Some(cleaned.to_string());
            }
        }
        // If first line has content, return it as version info
        if !words.is_empty() && line.len() < 80 {
            return Some(line.to_string());
        }
    }
    None
}

pub fn detect_tools() -> SystemStatus {
    let claude = detect_tool("claude", &["--version"]);
    // OpenAI Codex CLI
    let codex = detect_tool("codex", &["--version"]);

    SystemStatus { claude, codex }
}
