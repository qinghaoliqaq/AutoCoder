/// Workspace management — create per-task project dirs on the Desktop
/// and build a file-tree snapshot for the frontend file explorer.
use serde::Serialize;
use std::path::{Path, PathBuf};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ProjectDocs {
    /// Concatenated content of all discovered document files.
    pub content: String,
    /// File names found (relative to the workspace root), in discovery order.
    pub filenames: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Create `~/Desktop/<dir>/` and return its absolute path.
/// `dir_name` is an optional caller-supplied directory hint kept for
/// compatibility with legacy/manual flows; otherwise the name is derived from
/// the task description.
#[tauri::command]
pub fn create_workspace(task: String, dir_name: Option<String>) -> Result<String, String> {
    let desktop = dirs::desktop_dir().ok_or("Cannot locate Desktop directory")?;

    let dir_name = dir_name
        .map(|d| sanitize_name(&d)) // Director-supplied name wins
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| sanitize_name(&task)); // fallback: derive from task
    let workspace = desktop.join(&dir_name);
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Cannot create workspace '{dir_name}': {e}"))?;

    Ok(workspace.to_string_lossy().into_owned())
}

/// Scan a workspace directory for documentation files and return their concatenated content.
///
/// Searches (in order):
///   1. Well-known names in the root: PLAN.md, README.md, spec.md, requirements.md,
///      design.md, architecture.md  (case-insensitive match)
///   2. Every *.md file inside a `docs/` or `doc/` sub-directory
///
/// Files are included at most once.  Total content is capped at 400 KB to stay
/// within reasonable LLM context limits.
#[tauri::command]
pub fn read_project_docs(path: String) -> Result<ProjectDocs, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    const WELL_KNOWN: &[&str] = &[
        "plan.md",
        "readme.md",
        "spec.md",
        "requirements.md",
        "design.md",
        "architecture.md",
    ];
    const MAX_BYTES: usize = 400 * 1024;

    let mut filenames: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut total: usize = 0;

    // Helper: try to append a file if it hasn't been added yet and fits within cap
    let try_add = |rel: String,
                   full: &PathBuf,
                   filenames: &mut Vec<String>,
                   parts: &mut Vec<String>,
                   total: &mut usize| {
        if filenames.contains(&rel) {
            return;
        }
        let Ok(content) = std::fs::read_to_string(full) else {
            return;
        };
        if content.trim().is_empty() {
            return;
        }
        let remaining = MAX_BYTES.saturating_sub(*total);
        if remaining == 0 {
            return;
        }
        let chunk = if content.len() > remaining {
            content[..remaining].to_string()
        } else {
            content
        };
        *total += chunk.len();
        parts.push(format!("<!-- {} -->\n{}", rel, chunk));
        filenames.push(rel);
    };

    // 1. Well-known files in the root
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut root_files: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if !p.is_file() {
                    return None;
                }
                let name = e.file_name().to_string_lossy().to_lowercase();
                if WELL_KNOWN.contains(&name.as_str()) {
                    Some((e.file_name().to_string_lossy().into_owned(), p))
                } else {
                    None
                }
            })
            .collect();
        // Sort by WELL_KNOWN order so PLAN.md always comes first
        root_files.sort_by_key(|(name, _)| {
            WELL_KNOWN
                .iter()
                .position(|&w| w == name.to_lowercase().as_str())
                .unwrap_or(99)
        });
        for (name, full_path) in root_files {
            try_add(name, &full_path, &mut filenames, &mut parts, &mut total);
        }
    }

    // 2. *.md files inside docs/ or doc/
    for sub in &["docs", "doc"] {
        let dir = root.join(sub);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut doc_files: Vec<(String, PathBuf)> = entries
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if !p.is_file() {
                        return None;
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.to_lowercase().ends_with(".md") {
                        Some((format!("{sub}/{name}"), p))
                    } else {
                        None
                    }
                })
                .collect();
            doc_files.sort_by(|a, b| a.0.cmp(&b.0));
            for (rel, full_path) in doc_files {
                try_add(rel, &full_path, &mut filenames, &mut parts, &mut total);
            }
        }
    }

    Ok(ProjectDocs {
        content: parts.join("\n\n---\n\n"),
        filenames,
    })
}

/// Read an arbitrary file inside the workspace safely.
/// Validates that the file exists and is actually inside the workspace directory
/// to prevent path traversal attacks.
#[tauri::command]
pub fn read_workspace_file(path: String, relative_path: String) -> Result<String, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Workspace is not a directory: {path}"));
    }

    let target = root.join(&relative_path);

    let canonical_root =
        std::fs::canonicalize(&root).map_err(|e| format!("Invalid workspace path: {e}"))?;

    let canonical_target = std::fs::canonicalize(&target)
        .map_err(|e| format!("Cannot resolve target path {relative_path}: {e}"))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err("Path traversal denied".to_string());
    }

    std::fs::read_to_string(&canonical_target)
        .map_err(|e| format!("Cannot read file {relative_path}: {e}"))
}

/// Validate that `path` is an existing directory and return it as-is.
#[tauri::command]
pub fn open_project(path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    Ok(path)
}

/// Return a recursive file tree of `path` (max 5 levels deep).
#[tauri::command]
pub fn workspace_tree(path: String) -> Result<Vec<FileNode>, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    Ok(build_tree(&root, 5))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_tree(dir: &Path, depth: usize) -> Vec<FileNode> {
    if depth == 0 {
        return vec![];
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };

    let mut nodes: Vec<FileNode> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            // Skip hidden files and noisy directories
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "node_modules" | "__pycache__" | "target" | ".git" | "dist" | ".next"
                )
            {
                return None;
            }

            let is_dir = path.is_dir();
            let children = if is_dir {
                build_tree(&path, depth - 1)
            } else {
                vec![]
            };

            Some(FileNode {
                name,
                path: path.to_string_lossy().into_owned(),
                is_dir,
                children,
            })
        })
        .collect();

    // Directories first, then files; both sorted alphabetically
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    nodes
}

/// Convert a task description into a safe ASCII directory name.
/// "JWT 登录功能" → "jwt"  |  "build a todo app" → "build-a-todo-app"
fn sanitize_name(task: &str) -> String {
    let s: String = task
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    let s = s.trim_matches('-').to_lowercase();
    // Collapse consecutive hyphens
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    // Fallback if nothing ASCII survives
    let s = if s.is_empty() {
        "project".to_string()
    } else {
        s
    };
    s.chars().take(48).collect()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_name ────────────────────────────────────────────────────────

    #[test]
    fn sanitize_ascii_task() {
        assert_eq!(sanitize_name("build a todo app"), "build-a-todo-app");
    }

    #[test]
    fn sanitize_strips_non_ascii() {
        assert_eq!(sanitize_name("JWT 登录功能"), "jwt");
    }

    #[test]
    fn sanitize_collapses_hyphens() {
        assert_eq!(sanitize_name("hello---world"), "hello-world");
    }

    #[test]
    fn sanitize_fallback_on_empty() {
        assert_eq!(sanitize_name("你好世界"), "project");
    }

    #[test]
    fn sanitize_truncates_long_names() {
        let long = "a".repeat(100);
        assert_eq!(sanitize_name(&long).len(), 48);
    }

    #[test]
    fn sanitize_preserves_underscores() {
        assert_eq!(sanitize_name("my_feature_branch"), "my_feature_branch");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_name("  hello  "), "hello");
    }

    // ── build_tree ───────────────────────────────────────────────────────────

    #[test]
    fn build_tree_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".hidden")).unwrap();
        std::fs::write(tmp.path().join("visible.txt"), "content").unwrap();

        let tree = build_tree(tmp.path(), 1);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "visible.txt");
    }

    #[test]
    fn build_tree_skips_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("node_modules")).unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();

        let tree = build_tree(tmp.path(), 1);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "src");
    }

    #[test]
    fn build_tree_respects_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("deep.txt"), "content").unwrap();

        let tree = build_tree(tmp.path(), 1);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        // Depth 1: "a" has no children explored
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn build_tree_sorts_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("b.txt"), "").unwrap();
        std::fs::create_dir(tmp.path().join("a_dir")).unwrap();
        std::fs::write(tmp.path().join("a.txt"), "").unwrap();

        let tree = build_tree(tmp.path(), 1);
        assert!(tree[0].is_dir, "directories should sort first");
        assert_eq!(tree[0].name, "a_dir");
    }

    // ── read_workspace_file (path traversal) ─────────────────────────────────

    #[test]
    fn read_workspace_file_blocks_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("secret.txt"), "secret").unwrap();
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("ok.txt"), "ok").unwrap();

        let result = read_workspace_file(
            workspace.to_string_lossy().into_owned(),
            "../secret.txt".to_string(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("traversal") || err.contains("resolve")
        );
    }

    #[test]
    fn read_workspace_file_allows_valid_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("hello.txt"), "hello").unwrap();

        let result = read_workspace_file(
            tmp.path().to_string_lossy().into_owned(),
            "hello.txt".to_string(),
        );
        assert_eq!(result.unwrap(), "hello");
    }

    // ── open_project ─────────────────────────────────────────────────────────

    #[test]
    fn open_project_rejects_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        let result = open_project(file.to_string_lossy().into_owned());
        assert!(result.is_err());
    }

    #[test]
    fn open_project_accepts_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = open_project(tmp.path().to_string_lossy().into_owned());
        assert!(result.is_ok());
    }
}
