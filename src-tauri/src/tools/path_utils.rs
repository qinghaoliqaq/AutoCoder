/// Shared path resolution & security helpers.
///
/// Used by file_read, file_edit, file_write, and other tools that need to
/// resolve user-supplied paths safely within the workspace boundary.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Hard cap on per-stream output captured from a subprocess (256 KiB).
/// `Command::output()` buffers the entire stdout/stderr into memory, so
/// a runaway producer (`yes`, `cat huge_file`, accidental log explosion)
/// would otherwise OOM the process.  Bash/REPL/PowerShell all share this
/// limit via [`capture_stream`].
pub const MAX_STREAM_BYTES: usize = 256 * 1024;

/// Truncate raw command output bytes to a UTF-8 string of at most
/// [`MAX_STREAM_BYTES`], appending a truncation marker when cut.  Splits
/// on a UTF-8 char boundary so we never hand the model half a codepoint.
pub fn capture_stream(bytes: &[u8], label: &str) -> String {
    if bytes.len() <= MAX_STREAM_BYTES {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    // Find the largest valid UTF-8 prefix at or below MAX_STREAM_BYTES.
    // `from_utf8_lossy` then replaces only any truly invalid trailing
    // partial codepoint with a replacement char — correct behaviour.
    let mut end = MAX_STREAM_BYTES;
    while end > 0 && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    let head = String::from_utf8_lossy(&bytes[..end]);
    format!(
        "{head}\n[... {label} truncated: captured {kept} of {total} bytes ...]",
        kept = end,
        total = bytes.len(),
    )
}

/// Atomically write `data` to `path`.
///
/// Strategy: write to a uniquely-named sibling `.tmp` then rename into
/// place.  This prevents partial-write corruption on disk-full or power
/// loss, and keeps concurrent writers from trampling each other's
/// staging files (the tmp name includes pid + nanoseconds).
///
/// On Windows `rename` fails if the destination exists, so we fall
/// back to remove-then-rename.  If the remove succeeds but the rename
/// fails, the tmp file is left on disk so the caller can recover
/// rather than being left with neither version present.
///
/// Creates parent directories as needed.
pub async fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
    }
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let ext = path
        .extension()
        .map(|e| format!("{}.{pid}.{ns}.tmp", e.to_string_lossy()))
        .unwrap_or_else(|| format!("{pid}.{ns}.tmp"));
    let tmp = path.with_extension(ext);
    tokio::fs::write(&tmp, data)
        .await
        .map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Windows: destination must be removed first.  Keep tmp intact
            // if remove_file fails so we never end up with neither file.
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| format!("Cannot remove old {}: {e}", path.display()))?;
            tokio::fs::rename(&tmp, path)
                .await
                .map_err(|e| format!("Cannot rename {}: {e}", path.display()))
        }
        Err(e) => Err(format!("Cannot rename {}: {e}", path.display())),
    }
}

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

    #[tokio::test]
    async fn atomic_write_creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("deep/nested/file.txt");
        atomic_write(&target, b"hello").await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn atomic_write_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("file.txt");
        std::fs::write(&target, b"old").unwrap();
        atomic_write(&target, b"new").await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[tokio::test]
    async fn atomic_write_leaves_no_tmp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("file.txt");
        atomic_write(&target, b"content").await.unwrap();
        let stray_tmp: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(stray_tmp.is_empty(), "tmp files leaked: {stray_tmp:?}");
    }
}
