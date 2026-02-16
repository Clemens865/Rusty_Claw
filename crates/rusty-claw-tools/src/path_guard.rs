//! Path validation to ensure tools operate within the workspace.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// Validate and resolve a path, ensuring it's within the workspace.
pub fn validate_path(raw: &str, workspace: &Path, restrict: bool) -> Result<PathBuf> {
    let path = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        workspace.join(raw)
    };

    // Canonicalize to resolve symlinks and ..
    let canonical = path.canonicalize().unwrap_or_else(|_| {
        // If file doesn't exist yet, normalize the parent
        if let Some(parent) = path.parent() {
            if let Ok(canon_parent) = parent.canonicalize() {
                return canon_parent.join(path.file_name().unwrap_or_default());
            }
        }
        path.clone()
    });

    if restrict {
        let workspace_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        if !canonical.starts_with(&workspace_canon) {
            bail!(
                "Path '{}' is outside the workspace '{}'",
                canonical.display(),
                workspace_canon.display()
            );
        }
    }

    Ok(canonical)
}

/// Validate a path for writing, creating parent dirs if needed.
pub fn validate_write_path(
    raw: &str,
    workspace: &Path,
    restrict: bool,
    create_dirs: bool,
) -> Result<PathBuf> {
    let path = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        workspace.join(raw)
    };

    if restrict {
        let workspace_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());

        // For new files, walk up to find the nearest existing ancestor
        let check_path = if path.exists() {
            path.canonicalize()?
        } else {
            // Find the deepest existing ancestor and canonicalize from there
            let mut ancestor = path.clone();
            while !ancestor.exists() {
                if let Some(parent) = ancestor.parent() {
                    ancestor = parent.to_path_buf();
                } else {
                    break;
                }
            }
            if ancestor.exists() {
                let canon_ancestor = ancestor.canonicalize()?;
                let suffix = path.strip_prefix(&ancestor).unwrap_or(path.as_path());
                canon_ancestor.join(suffix)
            } else if !create_dirs {
                bail!("Parent directory does not exist: {}", path.display());
            } else {
                path.clone()
            }
        };

        if !check_path.starts_with(&workspace_canon) {
            bail!(
                "Path '{}' is outside the workspace '{}'",
                check_path.display(),
                workspace_canon.display()
            );
        }
    }

    if create_dirs {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_path_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("test.txt"), "hello").unwrap();

        let result = validate_path("test.txt", workspace, true).unwrap();
        assert!(result.starts_with(workspace.canonicalize().unwrap()));
    }

    #[test]
    fn test_escape_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::create_dir_all(workspace.join("sub")).unwrap();
        std::fs::write(workspace.join("sub/file.txt"), "x").unwrap();

        // Trying to escape via ..
        let result = validate_path("../../etc/passwd", workspace, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_unrestricted_allows_escape() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // When not restricted, any path is ok
        let result = validate_path("/tmp", workspace, false);
        assert!(result.is_ok());
    }
}
