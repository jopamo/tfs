use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Resolve a path relative to root, ensuring it stays within root.
pub fn resolve_path(root: &Path, path: &Path) -> Result<PathBuf> {
    // If path is absolute, ensure it's within root.
    // If path is relative, join with root.
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    // Normalize and check for escapes.
    let canonical = resolved.canonicalize().map_err(|_| {
        anyhow::anyhow!("path does not exist or cannot be canonicalized: {}", resolved.display())
    })?;
    let root_canonical = root.canonicalize().map_err(|_| {
        anyhow::anyhow!("root does not exist or cannot be canonicalized: {}", root.display())
    })?;
    if !canonical.starts_with(&root_canonical) {
        bail!("path escapes root: {} -> {}", path.display(), canonical.display());
    }
    Ok(canonical)
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