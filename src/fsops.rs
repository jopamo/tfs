use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Result of a filesystem operation.
pub struct OpResult {
    pub bytes_copied: u64,
    pub final_dst: PathBuf,
    pub overwritten: bool,
    pub backup_path: Option<PathBuf>,
}

/// Create a directory.
pub fn mkdir(dst: &Path, parents: bool) -> Result<()> {
    if parents {
        std::fs::create_dir_all(dst)?;
    } else {
        std::fs::create_dir(dst)?;
    }
    Ok(())
}

/// Check if two paths are on the same filesystem.
#[cfg(unix)]
fn same_filesystem(src: &Path, dst: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let src_meta = std::fs::metadata(src).context("failed to stat source")?;
    let dst_parent = dst.parent().unwrap_or_else(|| Path::new("."));
    let dst_parent_meta =
        std::fs::metadata(dst_parent).context("failed to stat destination parent")?;
    Ok(src_meta.dev() == dst_parent_meta.dev())
}

#[cfg(windows)]
fn same_filesystem(_src: &Path, _dst: &Path) -> Result<bool> {
    // volume_serial_number is unstable (feature `windows_by_handle`).
    // Fallback to copy+delete which is safe but slower.
    Ok(false)
}

#[cfg(not(any(unix, windows)))]
fn same_filesystem(_src: &Path, _dst: &Path) -> Result<bool> {
    Ok(false)
}

/// Move a file or directory.
pub fn mv(src: &Path, dst: &Path, cross_device: bool) -> Result<OpResult> {
    let same_fs = same_filesystem(src, dst)?;
    if same_fs && !cross_device {
        // Atomic rename within same filesystem
        std::fs::rename(src, dst)?;
        Ok(OpResult {
            bytes_copied: 0,
            final_dst: dst.to_path_buf(),
            overwritten: false, // rename fails if destination exists
            backup_path: None,
        })
    } else {
        // Cross‑device or forced copy+delete
        let metadata = std::fs::metadata(src)?;
        let bytes = cp(src, dst, true)?.bytes_copied;
        if metadata.is_file() {
            std::fs::remove_file(src)?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(src)?;
        }
        Ok(OpResult {
            bytes_copied: bytes,
            final_dst: dst.to_path_buf(),
            overwritten: false,
            backup_path: None,
        })
    }
}

/// Copy a file or directory.
pub fn cp(src: &Path, dst: &Path, recursive: bool) -> Result<OpResult> {
    let metadata = std::fs::metadata(src).context("source not found")?;
    if metadata.is_file() {
        let bytes = std::fs::copy(src, dst).context("copy failed")?;
        Ok(OpResult {
            bytes_copied: bytes,
            final_dst: dst.to_path_buf(),
            overwritten: false,
            backup_path: None,
        })
    } else if metadata.is_dir() {
        if !recursive {
            anyhow::bail!("cannot copy directory without recursive=true");
        }
        // Manual recursive copy using walkdir
        // 1. Create destination directory
        if !dst.exists() {
            std::fs::create_dir_all(dst)?;
        }

        let mut bytes = 0;
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry?;
            let rel_path = entry.path().strip_prefix(src)?;
            let target_path = dst.join(rel_path);

            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target_path)?;
            } else {
                let copied = std::fs::copy(entry.path(), &target_path)?;
                bytes += copied;
            }
        }

        Ok(OpResult {
            bytes_copied: bytes,
            final_dst: dst.to_path_buf(),
            overwritten: false,
            backup_path: None,
        })
    } else {
        anyhow::bail!("unsupported file type: {:?}", metadata.file_type());
    }
}

/// Trash a file (move to quarantine directory).
pub fn trash(src: &Path) -> Result<OpResult> {
    // TODO: implement proper trash location
    let dst = src.with_extension("trash");
    mv(src, &dst, false)
}
