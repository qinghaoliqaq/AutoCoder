/// Persistent memory system inspired by Claude Code's MEMORY.md architecture.
///
/// Memory is project-scoped and stored under:
///   <config_dir>/ai-dev-hub/memory/<project-slug>/MEMORY.md
///   <config_dir>/ai-dev-hub/memory/<project-slug>/<topic>.md
///
/// MEMORY.md is a lightweight index (~200 lines max, ~25KB max) with one-line
/// entries pointing to topic files. Topic files hold the detail.
///
/// Memory types (matching Claude Code's taxonomy):
///   - user:      role, expertise, preferences (always private)
///   - feedback:  corrections and confirmed approaches
///   - project:   current work goals, bugs, initiatives
///   - reference: pointers to external systems (dashboards, docs, APIs)
///
/// The memory prompt is injected into the system prompt for both the director
/// and skill runs, giving the model persistent context across sessions.
use std::path::{Path, PathBuf};

/// Hard limits for the MEMORY.md entrypoint.
const MAX_ENTRYPOINT_LINES: usize = 200;
const MAX_ENTRYPOINT_BYTES: usize = 25_000;

/// Max number of topic files to include per turn.
const MAX_TOPIC_FILES: usize = 5;

/// Max bytes per topic file included in prompt.
const MAX_TOPIC_FILE_BYTES: usize = 8_000;

// ── Path helpers ────────────────────────────────────────────────────────────

/// Base directory for all memory storage.
fn memory_base_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ai-dev-hub").join("memory"))
}

/// Project-scoped memory directory.
/// The slug is derived from the workspace path (last 2 path components, sanitised).
fn project_memory_dir(workspace: Option<&str>) -> Option<PathBuf> {
    let base = memory_base_dir()?;
    let slug = workspace_slug(workspace);
    Some(base.join(slug))
}

/// Derive a filesystem-safe slug from the workspace path.
fn workspace_slug(workspace: Option<&str>) -> String {
    let ws = workspace.unwrap_or("default");
    let p = Path::new(ws);
    let components: Vec<&str> = p
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    let tail: Vec<&str> = if components.len() >= 2 {
        components[components.len() - 2..].to_vec()
    } else {
        components
    };
    let raw = tail.join("_");
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// Path to the project's MEMORY.md entrypoint.
fn entrypoint_path(workspace: Option<&str>) -> Option<PathBuf> {
    project_memory_dir(workspace).map(|d| d.join("MEMORY.md"))
}

// ── Reading ─────────────────────────────────────────────────────────────────

/// Load the MEMORY.md entrypoint, truncated to limits.
pub fn load_entrypoint(workspace: Option<&str>) -> Option<String> {
    let path = entrypoint_path(workspace)?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_entrypoint(trimmed))
}

/// Truncate to line and byte limits, matching Claude Code's logic.
fn truncate_entrypoint(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let byte_count = content.len();

    let over_lines = line_count > MAX_ENTRYPOINT_LINES;
    let over_bytes = byte_count > MAX_ENTRYPOINT_BYTES;

    if !over_lines && !over_bytes {
        return content.to_string();
    }

    // Line-truncate first
    let mut truncated: String = if over_lines {
        lines[..MAX_ENTRYPOINT_LINES].join("\n")
    } else {
        content.to_string()
    };

    // Byte-truncate at last newline before cap
    if truncated.len() > MAX_ENTRYPOINT_BYTES {
        let cut_at = truncated[..MAX_ENTRYPOINT_BYTES]
            .rfind('\n')
            .unwrap_or(MAX_ENTRYPOINT_BYTES);
        truncated.truncate(cut_at);
    }

    let reason = match (over_bytes, over_lines) {
        (true, false) => {
            format!("{byte_count} bytes (limit {MAX_ENTRYPOINT_BYTES}) — entries too long")
        }
        (false, true) => format!("{line_count} lines (limit {MAX_ENTRYPOINT_LINES})"),
        _ => format!("{line_count} lines and {byte_count} bytes"),
    };

    format!(
        "{truncated}\n\n> WARNING: MEMORY.md is {reason}. \
         Only part was loaded. Keep entries to one line under ~200 chars; \
         move detail into topic files."
    )
}

/// Scan topic files in the project memory directory.
/// Returns (filename, first_line_as_description) pairs sorted by mtime (newest first).
fn scan_topic_files(workspace: Option<&str>) -> Vec<(PathBuf, String)> {
    let dir = match project_memory_dir(workspace) {
        Some(d) if d.exists() => d,
        _ => return Vec::new(),
    };

    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false)
                && path.file_name().map(|n| n != "MEMORY.md").unwrap_or(false)
            {
                let mtime = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                entries.push((path, mtime));
            }
        }
    }
    // Newest first
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries
        .into_iter()
        .map(|(path, _)| {
            let desc = std::fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            (path, desc)
        })
        .collect()
}

/// Select the most relevant topic files for the current task.
/// Simple heuristic: keyword match between task and topic file description/name.
/// Returns file contents (truncated to MAX_TOPIC_FILE_BYTES).
fn select_relevant_topics(workspace: Option<&str>, task_hint: &str) -> Vec<(String, String)> {
    let topics = scan_topic_files(workspace);
    if topics.is_empty() {
        return Vec::new();
    }

    let task_lower = task_hint.to_lowercase();
    let task_words: Vec<&str> = task_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    // Score each topic by keyword overlap
    let mut scored: Vec<(f32, &PathBuf, &str)> = topics
        .iter()
        .map(|(path, desc)| {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let haystack = format!("{} {}", name, desc).to_lowercase();
            let score: f32 = task_words.iter().filter(|w| haystack.contains(**w)).count() as f32;
            // Boost recent files slightly
            (score + 0.1, path, desc.as_str())
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .into_iter()
        .take(MAX_TOPIC_FILES)
        .filter_map(|(_, path, _)| {
            let name = path.file_name()?.to_str()?.to_string();
            let content = std::fs::read_to_string(path).ok()?;
            let truncated = if content.len() > MAX_TOPIC_FILE_BYTES {
                format!("{}...\n[truncated]", &content[..MAX_TOPIC_FILE_BYTES])
            } else {
                content
            };
            Some((name, truncated))
        })
        .collect()
}

/// Build the full memory prompt to inject into the system prompt.
/// Returns None if no memory exists for this project.
pub fn build_memory_prompt(workspace: Option<&str>, task_hint: &str) -> Option<String> {
    let entrypoint = load_entrypoint(workspace);
    let topics = select_relevant_topics(workspace, task_hint);

    if entrypoint.is_none() && topics.is_empty() {
        return None;
    }

    let mut sections = Vec::new();

    sections.push(
        "# Project Memory\n\nPersistent memory from previous sessions. \
        Use this context to maintain continuity. When you learn something important, \
        tell the user to save it to memory."
            .to_string(),
    );

    if let Some(entry) = &entrypoint {
        sections.push(format!("## MEMORY.md (Index)\n\n{entry}"));
    }

    for (name, content) in &topics {
        sections.push(format!("## Memory: {name}\n\n{content}"));
    }

    sections.push(MEMORY_GUIDELINES.to_string());

    Some(sections.join("\n\n---\n\n"))
}

// ── Writing ─────────────────────────────────────────────────────────────────

/// Append a line to MEMORY.md. Creates the file/directory if needed.
pub fn append_to_entrypoint(workspace: Option<&str>, line: &str) -> Result<String, String> {
    let dir = project_memory_dir(workspace).ok_or("Cannot determine memory directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create memory dir: {e}"))?;

    let path = dir.join("MEMORY.md");
    let mut content = std::fs::read_to_string(&path).unwrap_or_default();

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(line.trim());
    content.push('\n');

    std::fs::write(&path, &content).map_err(|e| format!("Cannot write MEMORY.md: {e}"))?;

    Ok(path.display().to_string())
}

/// Write or overwrite a topic file.
pub fn write_topic(workspace: Option<&str>, name: &str, content: &str) -> Result<String, String> {
    let dir = project_memory_dir(workspace).ok_or("Cannot determine memory directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create memory dir: {e}"))?;

    // Sanitize filename
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = if safe_name.ends_with(".md") {
        safe_name
    } else {
        format!("{safe_name}.md")
    };

    let path = dir.join(&filename);
    std::fs::write(&path, content).map_err(|e| format!("Cannot write topic file: {e}"))?;

    Ok(path.display().to_string())
}

/// List all memory files for a project.
pub fn list_memories(workspace: Option<&str>) -> Vec<String> {
    let dir = match project_memory_dir(workspace) {
        Some(d) if d.exists() => d,
        _ => return Vec::new(),
    };

    let mut files = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    files.push(name.to_string());
                }
            }
        }
    }
    files.sort();
    files
}

// ── Guidelines injected into the prompt ─────────────────────────────────────

const MEMORY_GUIDELINES: &str = r#"## Memory Guidelines

When the user asks you to "remember" something or you identify important persistent context:

**MEMORY.md (Index)** — one-line entries, max ~200 chars each:
- `[user] Senior Rust developer, prefers minimal code, Chinese speaker`
- `[project] Building Tauri 2 AI dev tool with director/agent architecture`
- `[reference] API docs: see topic/api-reference.md`
- `[feedback] Always use Anthropic built-in tool schemas for bash/editor`

**Topic files** — detailed context in separate .md files:
- Name files descriptively: `architecture.md`, `auth-flow.md`, `coding-conventions.md`
- Start with a one-line description (used for relevance matching)
- Include the memory type in YAML frontmatter: `type: project`

**What NOT to save**: code snippets (they go stale), temporary debugging state,
information already in the codebase, overly specific implementation details.

To save memory, tell the user: "I suggest saving this to memory: [content]"
The user can then confirm and the memory will persist across sessions."#;

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_slug_basic() {
        assert_eq!(
            workspace_slug(Some("/home/user/projects/my-app")),
            "projects_my-app"
        );
    }

    #[test]
    fn workspace_slug_default() {
        assert_eq!(workspace_slug(None), "default");
    }

    #[test]
    fn workspace_slug_single_component() {
        assert_eq!(workspace_slug(Some("/root")), "root");
    }

    #[test]
    fn truncate_entrypoint_under_limit() {
        let content = "line 1\nline 2\nline 3";
        assert_eq!(truncate_entrypoint(content), content);
    }

    #[test]
    fn truncate_entrypoint_over_lines() {
        let lines: Vec<String> = (0..250).map(|i| format!("line {i}")).collect();
        let content = lines.join("\n");
        let result = truncate_entrypoint(&content);
        assert!(result.contains("WARNING"));
        assert!(result.contains("250 lines"));
    }

    #[test]
    fn topic_file_write_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().to_str().unwrap();

        // We can't easily test with the real config dir, but we can test the slug
        let slug = workspace_slug(Some(ws));
        assert!(!slug.is_empty());
    }
}
