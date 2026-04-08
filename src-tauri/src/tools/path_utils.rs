/// Shared path resolution & security helpers.
///
/// Used by file_read, file_edit, file_write, and other tools that need to
/// resolve user-supplied paths safely within the workspace boundary.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Resolve a user-supplied path relative to the workspace, ensuring it does
/// not escape the workspace boundary. Handles symlinks, `..`, and `.` components.
pub fn resolve_path(path_str: &str, workspace: &Path) -> Result<PathBuf, String> {
    let p = Path::new(path_str);
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace.join(p)
    };
    let normalized = normalize_lexical_path(&resolved)?;
    let canonical = canonicalize_with_virtual_tail(&normalized)?;
    let ws_canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    if !canonical.starts_with(&ws_canonical) {
        return Err(format!(
            "path '{}' escapes workspace boundary '{}'",
            path_str,
            ws_canonical.display()
        ));
    }
    Ok(canonical)
}

/// Lexically normalize a path (resolve `.` and `..` without hitting the filesystem).
fn normalize_lexical_path(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::Normal(segment) => normalized.push(segment),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!("path error: '{}' escapes root", path.display()));
                }
            }
        }
    }
    Ok(normalized)
}

/// Canonicalize the deepest existing ancestor of a path, then re-attach
/// the non-existent tail. This handles paths where the final file or
/// intermediate directories do not yet exist.
fn canonicalize_with_virtual_tail(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path.canonicalize().map_err(|e| format!("path error: {e}"));
    }

    let mut ancestor = path;
    let mut tail: Vec<OsString> = Vec::new();
    while !ancestor.exists() {
        let Some(name) = ancestor.file_name() else {
            return Err(format!(
                "path error: no existing ancestor for '{}'",
                path.display()
            ));
        };
        tail.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| format!("path error: no existing ancestor for '{}'", path.display()))?;
    }

    let mut canonical = ancestor
        .canonicalize()
        .map_err(|e| format!("path error: {e}"))?;
    for segment in tail.iter().rev() {
        canonical.push(segment);
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_blocks_workspace_escape() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let err = resolve_path("../outside/new.txt", &workspace).unwrap_err();
        assert!(err.contains("escapes workspace boundary"));
    }

    #[test]
    fn resolve_path_allows_normalized_relative() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let resolved = resolve_path("src/../src/new.txt", &workspace).unwrap();
        assert!(resolved.starts_with(&workspace));
    }
}
