use anyhow::Result;
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
    if let Some(dst) = dst
        && let crate::model::Operation::Mkdir {
            parents: mkdir_parents,
            ..
        } = op
        && *mkdir_parents
    {
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
                // Check for symlinks BEFORE canonicalization resolution to catch them
                // We use resolve_path to ensure it doesn't escape, but we also check the raw path for policy
                // Better: use normalize_lexical logic if exposed, or just simple check if it doesn't have ..?
                // But src might be relative.
                // Let's rely on resolve_path returning the canonical path for EXISTENCE/SAFETY.
                // But for SYMLINK check, we need the path that points TO the symlink.
                // If `src` is "link", `root.join(src)` is ".../link".
                // We should check metadata of THAT.
                // CAUTION: If `src` escapes root via `..`, `root.join` is unsafe?
                // `resolve_path` checks for escape. If `resolve_path` succeeds, then `src` (resolved) is safe.
                // But `resolved` is canonical.
                // We need to verify `root.join(src)` is safe AND is the symlink.

                // Let's do:
                let resolved = crate::resolve::resolve_path(&plan.root, src)?;
                if !resolved.exists() {
                    anyhow::bail!("source does not exist: {}", resolved.display());
                }

                // Check symlink policy on the path segments?
                // Or just on the immediate file pointed to by `src` relative to root?
                // If `src` is "a/b", and "a" is a symlink?
                // Confinement usually implies we don't care if intermediates are symlinks as long as they stay in root?
                // `resolve_path` ensures confinement.
                // `SymlinkPolicy` usually targets the LEAF? Or any part?
                // Usually the file being operated on.

                // Construct path we think it is:
                let potential_link = plan.root.join(src);
                // Verify it exists (it might be `..` normalized out, or `.`?)
                // If we use `crate::resolve::resolve_path` without canonicalization?
                // `resolve_path` is hardcoded to canonicalize.

                // Let's try to check `symlink_metadata` on `potential_link`.
                // Note: `potential_link` might have `..`.
                // If we `canonicalize` potential_link, we lose the link.
                // We just want to know if it IS a link.
                // `std::fs::symlink_metadata` works on paths with `..`.

                if let Ok(meta) = std::fs::symlink_metadata(&potential_link)
                    && meta.file_type().is_symlink()
                {
                    // It is a symlink! Check policy.
                    crate::policy::handle_symlink(plan.symlink_policy, &potential_link)?;
                }

                // Also check `resolved` just in case (e.g. if src was "." and root was symlink?)
                // But `handle_symlink` on resolved (target) passes if target is file.
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn normalize_is_deterministic_across_runs() {
        let op = crate::model::Operation::Mkdir {
            dst: PathBuf::from("a/b"),
            parents: true,
        };
        // Use a dummy root that exists (tempdir) to avoid resolve error if it checks existence?
        // normalize_plan calls resolve_path. resolve_path checks canonicalization of root.
        // So we need a real root.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        let plan = crate::model::Plan {
            root: root.clone(),
            transaction: crate::model::TransactionMode::All,
            collision_policy: crate::model::CollisionPolicy::Fail,
            symlink_policy: crate::model::SymlinkPolicy::Error,
            allow_overwrite: false,
            operations: vec![op.clone()],
        };

        let a_ops = normalize_plan(&plan).unwrap();
        let b_ops = normalize_plan(&plan).unwrap();

        assert_eq!(a_ops.len(), b_ops.len());
        assert_eq!(a_ops.len(), 1);

        let a = &a_ops[0];
        let b = &b_ops[0];

        assert_eq!(format!("{:?}", a.op), format!("{:?}", b.op));
        assert_eq!(a.resolved_src, b.resolved_src);
        assert_eq!(a.resolved_dst, b.resolved_dst);
        assert_eq!(a.parents, b.parents);
    }

    #[test]
    fn test_compute_parent_dirs() {
        let op = crate::model::Operation::Mkdir {
            dst: PathBuf::from("a/b/c"),
            parents: true,
        };
        let _dst = Some(PathBuf::from("/root/a/b/c"));

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target = root.join("a/b/c");

        let parents = compute_parent_dirs(&Some(target.clone()), &op);

        assert_eq!(parents.len(), 2);
        assert_eq!(parents[0], root.join("a"));
        assert_eq!(parents[1], root.join("a/b"));

        std::fs::create_dir(root.join("a")).unwrap();
        let parents_2 = compute_parent_dirs(&Some(target), &op);
        assert_eq!(parents_2.len(), 1);
        assert_eq!(parents_2[0], root.join("a/b"));
    }
}
