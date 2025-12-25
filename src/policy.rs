use anyhow::{bail, Result};
use std::path::Path;

/// Check collision policy and compute final destination.
pub fn resolve_collision(
    policy: crate::model::CollisionPolicy,
    dst: &Path,
    allow_overwrite: bool,
) -> Result<(std::path::PathBuf, Option<std::path::PathBuf>)> {
    if !dst.exists() {
        return Ok((dst.to_path_buf(), None));
    }
    match policy {
        crate::model::CollisionPolicy::Fail => {
            bail!("destination already exists and policy is 'fail': {}", dst.display());
        }
        crate::model::CollisionPolicy::Suffix => {
            let mut counter = 2;
            loop {
                let candidate = dst.with_extension(format!(
                    "{}.{}",
                    dst.extension().and_then(|s| s.to_str()).unwrap_or(""),
                    counter
                ));
                if !candidate.exists() {
                    return Ok((candidate, None));
                }
                counter += 1;
            }
        }
        crate::model::CollisionPolicy::Hash8 => {
            // TODO: compute hash of file contents
            let hash = "deadbeef";
            let candidate = dst.with_extension(format!(
                "{}.{}",
                dst.extension().and_then(|s| s.to_str()).unwrap_or(""),
                hash
            ));
            Ok((candidate, None))
        }
        crate::model::CollisionPolicy::OverwriteWithBackup => {
            if !allow_overwrite {
                bail!("overwrite_with_backup policy requires --allow-overwrite flag");
            }
            let backup = dst.with_extension(format!(
                "{}.backup",
                dst.extension().and_then(|s| s.to_str()).unwrap_or("")
            ));
            // TODO: actually move existing file to backup
            Ok((dst.to_path_buf(), Some(backup)))
        }
    }
}

/// Apply symlink policy.
pub fn handle_symlink(
    policy: crate::model::SymlinkPolicy,
    path: &Path,
) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        match policy {
            crate::model::SymlinkPolicy::Follow => Ok(()),
            crate::model::SymlinkPolicy::Skip => bail!("symlink skipped: {}", path.display()),
            crate::model::SymlinkPolicy::Error => bail!("symlink not allowed: {}", path.display()),
        }
    } else {
        Ok(())
    }
}