use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

/// Normalize a path lexically (resolve `.` and `..` without accessing filesystem).
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

/// Resolve a path relative to root, ensuring it stays within root.
pub fn resolve_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("root error {}: {}", root.display(), e))?;

    // 1. Join path to root (or take absolute)
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root_canon.join(path)
    };

    // 2. Try to canonicalize directly (fast path for existing files)
    if let Ok(canon) = candidate.canonicalize() {
        if !canon.starts_with(&root_canon) {
            bail!(
                "path escapes root: {} -> {}",
                path.display(),
                canon.display()
            );
        }
        return Ok(canon);
    }

    // 3. Handle non-existent path
    // Find the longest existing prefix
    let mut current = candidate.clone();
    let mut suffix_components = Vec::new();

    while !current.exists() {
        if let Some(name) = current.file_name() {
            suffix_components.push(name.to_os_string());
            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }
        } else {
            // Reached root/empty?
            break;
        }
    }
    suffix_components.reverse();

    // Canonicalize the existing prefix
    let prefix_canon = current
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("path prefix does not exist: {}", current.display()))?;

    // Append suffix
    let mut final_path = prefix_canon;
    for component in suffix_components {
        final_path.push(component);
    }

    // Normalize lexically to resolve any `..` in the appended part
    let normalized = normalize_lexical(&final_path);
    let root_normalized = normalize_lexical(&root_canon); // likely same as root_canon but to be safe

    if !normalized.starts_with(&root_normalized) {
        bail!(
            "path escapes root: {} -> {}",
            path.display(),
            normalized.display()
        );
    }

    Ok(normalized)
}

/// Validate that all operations stay within root.
pub fn validate_root_confinement(plan: &crate::model::Plan) -> Result<()> {
    for op in &plan.operations {
        let paths = match op {
            crate::model::Operation::Mkdir { dst, .. } => vec![dst],
            crate::model::Operation::Move { src, dst, .. } => vec![src, dst],
            crate::model::Operation::Copy { src, dst, .. } => vec![src, dst],
            crate::model::Operation::Rename { src, dst } => vec![src, dst],
            crate::model::Operation::Trash { src } => vec![src],
        };
        for path in paths {
            resolve_path(&plan.root, path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_lexical() {
        assert_eq!(normalize_lexical(Path::new("a/b")), PathBuf::from("a/b"));
        assert_eq!(normalize_lexical(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_lexical(Path::new("a/../b")), PathBuf::from("b"));
        assert_eq!(normalize_lexical(Path::new("/a/../b")), PathBuf::from("/b"));
        assert_eq!(normalize_lexical(Path::new("..")), PathBuf::from(".")); // pop empty -> .? Wait, my logic: empty -> .
        // My logic: parent of empty is empty. if components empty, return ".".

        // Let's check my logic for ".."
        // components: ParentDir. pop. if empty, ignore?
        // Code: `components.pop()`
        // If empty, nothing happens.
        // So `..` -> empty -> return `.`
        assert_eq!(normalize_lexical(Path::new("..")), PathBuf::from("."));

        // `a/../../b`
        // push a. pop -> empty. pop -> empty. push b. -> b.
        assert_eq!(
            normalize_lexical(Path::new("a/../../b")),
            PathBuf::from("b")
        );
    }

    #[test]
    fn resolve_rejects_parent_escape() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // "../evil.txt"
        let path = std::path::Path::new("..").join("evil.txt");
        let err = resolve_path(root, &path).unwrap_err();
        assert!(err.to_string().contains("escapes root"));
    }

    #[test]
    fn resolve_rejects_absolute_src_outside_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside_path = outside_dir.path().join("x");

        let err = resolve_path(root, &outside_path).unwrap_err();
        assert!(err.to_string().contains("escapes root"));
    }

    #[test]
    fn resolve_rejects_symlink_escape_by_default() {
        // resolve_path canonicalizes. If the canonical path is outside root, it fails.
        // This enforces "canonical(path) ⊆ root".
        #[cfg(unix)]
        {
            let root_dir = tempfile::tempdir().unwrap();
            let root = root_dir.path();
            let outside_dir = tempfile::tempdir().unwrap();

            // Create a symlink in root pointing outside
            let link_path = root.join("link");
            std::os::unix::fs::symlink(outside_dir.path(), &link_path).unwrap();

            // Try to resolve "link/file"
            let target = std::path::Path::new("link/file");
            let err = resolve_path(root, target).unwrap_err();
            assert!(err.to_string().contains("escapes root"));
        }
    }
}
