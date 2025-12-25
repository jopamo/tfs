use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Normalized operation ready for execution.
pub struct NormalizedOp {
    pub id: uuid::Uuid,
    pub op: crate::model::Operation,
    pub resolved_src: Option<PathBuf>,
    pub resolved_dst: Option<PathBuf>,
    pub parents: Vec<PathBuf>, // directories that need to be created
}

/// Validate and normalize a plan into a deterministic operation stream.
pub fn normalize_plan(plan: &crate::model::Plan) -> Result<Vec<NormalizedOp>> {
    let mut normalized = Vec::new();
    for op in &plan.operations {
        let (resolved_src, resolved_dst) = resolve_operation_paths(&plan.root, op)?;
        let parents = compute_parent_dirs(&resolved_dst, op);
        normalized.push(NormalizedOp {
            id: uuid::Uuid::new_v4(),
            op: op.clone(),
            resolved_src,
            resolved_dst,
            parents,
        });
    }
    // Ensure deterministic ordering (already same as input)
    Ok(normalized)
}

fn resolve_operation_paths(
    root: &Path,
    op: &crate::model::Operation,
) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    match op {
        crate::model::Operation::Mkdir { dst, .. } => {
            let resolved = crate::resolve::resolve_path(root, dst)?;
            Ok((None, Some(resolved)))
        }
        crate::model::Operation::Move { src, dst, .. } => {
            let resolved_src = crate::resolve::resolve_path(root, src)?;
            let resolved_dst = crate::resolve::resolve_path(root, dst)?;
            Ok((Some(resolved_src), Some(resolved_dst)))
        }
        crate::model::Operation::Copy { src, dst, .. } => {
            let resolved_src = crate::resolve::resolve_path(root, src)?;
            let resolved_dst = crate::resolve::resolve_path(root, dst)?;
            Ok((Some(resolved_src), Some(resolved_dst)))
        }
        crate::model::Operation::Rename { src, dst } => {
            let resolved_src = crate::resolve::resolve_path(root, src)?;
            let resolved_dst = crate::resolve::resolve_path(root, dst)?;
            Ok((Some(resolved_src), Some(resolved_dst)))
        }
        crate::model::Operation::Trash { src } => {
            let resolved_src = crate::resolve::resolve_path(root, src)?;
            Ok((Some(resolved_src), None))
        }
    }
}

fn compute_parent_dirs(dst: &Option<PathBuf>, op: &crate::model::Operation) -> Vec<PathBuf> {
    let mut parents = Vec::new();
    if let Some(dst) = dst {
        if let crate::model::Operation::Mkdir { parents: mkdir_parents, .. } = op {
            if *mkdir_parents {
                // Collect all parent directories that don't exist
                let mut path = dst.clone();
                while let Some(parent) = path.parent() {
                    if parent.exists() {
                        break;
                    }
                    parents.push(parent.to_path_buf());
                    path = parent.to_path_buf();
                }
                parents.reverse(); // create from outermost to innermost
            }
        }
    }
    parents
}

/// Pre‑flight checks (e.g., source existence, permissions, free space).
pub fn preflight_check(plan: &crate::model::Plan) -> Result<()> {
    for op in &plan.operations {
        match op {
            crate::model::Operation::Mkdir { .. } => {}
            crate::model::Operation::Move { src, .. }
            | crate::model::Operation::Copy { src, .. }
            | crate::model::Operation::Rename { src, .. }
            | crate::model::Operation::Trash { src } => {
                let resolved = crate::resolve::resolve_path(&plan.root, src)?;
                if !resolved.exists() {
                    anyhow::bail!("source does not exist: {}", resolved.display());
                }
                // Check symlink policy
                crate::policy::handle_symlink(plan.symlink_policy, &resolved)?;
            }
        }
    }
    Ok(())
}