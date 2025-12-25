use anyhow::{Context, Result};
use chrono::Utc;

/// Transaction manager for `all` or `op` mode.
pub struct TransactionManager {
    _mode: crate::model::TransactionMode,
    collision_policy: crate::model::CollisionPolicy,
    allow_overwrite: bool,
    journal_writer: Option<crate::journal::JournalWriter>,
    applied: Vec<crate::journal::JournalEntry>,
}

impl TransactionManager {
    pub fn new(
        mode: crate::model::TransactionMode,
        collision_policy: crate::model::CollisionPolicy,
        allow_overwrite: bool,
        journal_writer: Option<crate::journal::JournalWriter>,
    ) -> Self {
        Self {
            _mode: mode,
            collision_policy,
            allow_overwrite,
            journal_writer,
            applied: Vec::new(),
        }
    }

    /// Execute a single operation within the transaction.
    pub fn execute(&mut self, op: &crate::validate::NormalizedOp) -> Result<()> {
        // Write journal entry "start"
        let entry = crate::journal::JournalEntry {
            id: op.id,
            ts: Utc::now(),
            op: format!("{:?}", op.op),
            src: op.resolved_src.clone(),
            dst: op.resolved_dst.clone(),
            collision: None,
            status: crate::journal::JournalStatus::Start,
            undo: None,
        };
        self.write_journal(&entry)?;

        // Determine resolved paths (should be already resolved in normalized op)
        let src = op.resolved_src.as_deref();
        let dst_opt = op.resolved_dst.as_deref();

        // Handle collision policy if dst exists
        let mut final_dst_path = match dst_opt {
            Some(p) => p.to_path_buf(),
            None => std::path::PathBuf::new(),
        };
        let mut backup_path_opt = None;
        let mut collision_details = None;

        if let Some(dst) = dst_opt {
            // resolve_collision returns (final_dst, backup_path)
            let (resolved, backup) =
                crate::policy::resolve_collision(self.collision_policy, dst, self.allow_overwrite)?;

            if resolved != dst || backup.is_some() {
                collision_details = Some(crate::journal::CollisionDetails {
                    policy: self.collision_policy,
                    final_dst: resolved.clone(),
                    backup_path: backup.clone(),
                });
            }
            final_dst_path = resolved;
            backup_path_opt = backup;
        }

        // Perform backup if needed
        if let Some(backup) = &backup_path_opt {
            // We need to move the EXISTING dst to backup
            // dst_opt must be Some here
            let dst = dst_opt.unwrap();
            crate::fsops::mv(dst, backup, false).context("failed to create backup")?;
        }

        // Execute based on operation type
        match &op.op {
            crate::model::Operation::Mkdir {
                dst: dst_path,
                parents,
            } => {
                let dst = if op.resolved_dst.is_some() {
                    &final_dst_path
                } else {
                    dst_path.as_path()
                };
                crate::fsops::mkdir(dst, *parents)?;
                // Record undo metadata
                let undo = crate::journal::UndoMetadata::Mkdir {
                    created_dir: dst.to_path_buf(),
                };
                self.record_success(op.id, src, Some(dst), collision_details, Some(undo))?;
            }
            crate::model::Operation::Move {
                src: src_path,
                dst: dst_path,
                cross_device,
            } => {
                let src = src.unwrap_or(src_path.as_path());
                let dst = if op.resolved_dst.is_some() {
                    &final_dst_path
                } else {
                    dst_path.as_path()
                };
                let _result = crate::fsops::mv(src, dst, *cross_device)?;

                let undo = if let Some(bk) = backup_path_opt {
                    crate::journal::UndoMetadata::MoveWithOverwrite {
                        original_src: src.to_path_buf(),
                        backup_path: bk,
                    }
                } else {
                    crate::journal::UndoMetadata::Move {
                        original_src: src.to_path_buf(),
                    }
                };
                self.record_success(op.id, Some(src), Some(dst), collision_details, Some(undo))?;
            }
            crate::model::Operation::Copy {
                src: src_path,
                dst: dst_path,
                recursive,
            } => {
                let src = src.unwrap_or(src_path.as_path());
                let dst = if op.resolved_dst.is_some() {
                    &final_dst_path
                } else {
                    dst_path.as_path()
                };
                let _result = crate::fsops::cp(src, dst, *recursive)?;

                let undo = if let Some(bk) = backup_path_opt {
                    crate::journal::UndoMetadata::CopyWithOverwrite {
                        created_dst: dst.to_path_buf(),
                        backup_path: bk,
                    }
                } else {
                    crate::journal::UndoMetadata::Copy {
                        created_dst: dst.to_path_buf(),
                    }
                };
                self.record_success(op.id, Some(src), Some(dst), collision_details, Some(undo))?;
            }
            crate::model::Operation::Rename {
                src: src_path,
                dst: dst_path,
            } => {
                let src = src.unwrap_or(src_path.as_path());
                let dst = if op.resolved_dst.is_some() {
                    &final_dst_path
                } else {
                    dst_path.as_path()
                };
                let _result = crate::fsops::mv(src, dst, false)?;

                let undo = if let Some(bk) = backup_path_opt {
                    crate::journal::UndoMetadata::MoveWithOverwrite {
                        original_src: src.to_path_buf(),
                        backup_path: bk,
                    }
                } else {
                    crate::journal::UndoMetadata::Move {
                        original_src: src.to_path_buf(),
                    }
                };
                self.record_success(op.id, Some(src), Some(dst), collision_details, Some(undo))?;
            }
            crate::model::Operation::Trash { src: src_path } => {
                let src = src.unwrap_or(src_path.as_path());
                let result = crate::fsops::trash(src)?;
                let undo = crate::journal::UndoMetadata::Move {
                    original_src: src.to_path_buf(),
                };
                self.record_success(op.id, Some(src), Some(&result.final_dst), None, Some(undo))?;
            }
        }
        Ok(())
    }

    fn record_success(
        &mut self,
        id: uuid::Uuid,
        src: Option<&std::path::Path>,
        dst: Option<&std::path::Path>,
        collision: Option<crate::journal::CollisionDetails>,
        undo: Option<crate::journal::UndoMetadata>,
    ) -> Result<()> {
        let entry = crate::journal::JournalEntry {
            id,
            ts: Utc::now(),
            op: "".to_string(),
            src: src.map(|p| p.to_path_buf()),
            dst: dst.map(|p| p.to_path_buf()),
            collision,
            status: crate::journal::JournalStatus::Ok,
            undo,
        };
        self.write_journal(&entry)?;
        self.applied.push(entry);
        Ok(())
    }

    fn write_journal(&mut self, entry: &crate::journal::JournalEntry) -> Result<()> {
        if let Some(writer) = &mut self.journal_writer {
            writer.write(entry)?;
        }
        Ok(())
    }

    /// Commit the transaction (no-op for `all` mode after all ops succeed).
    pub fn commit(self) -> Result<()> {
        // TODO: mark journal as committed
        Ok(())
    }

    /// Rollback already applied operations.
    pub fn rollback(&mut self) -> Result<()> {
        // Take ownership of applied entries to avoid borrow conflicts
        let applied = std::mem::take(&mut self.applied);
        for entry in applied.iter().rev() {
            if let Some(undo) = &entry.undo {
                match undo {
                    crate::journal::UndoMetadata::Move { original_src } => {
                        let dst = entry.dst.as_ref().context("missing dst in journal")?;
                        crate::fsops::mv(dst, original_src, false)?;
                    }
                    crate::journal::UndoMetadata::Copy { created_dst } => {
                        std::fs::remove_file(created_dst)?;
                    }
                    crate::journal::UndoMetadata::Mkdir { created_dir } => {
                        std::fs::remove_dir(created_dir)?;
                    }
                    crate::journal::UndoMetadata::Overwrite { backup_path } => {
                        let dst = entry.dst.as_ref().context("missing dst in journal")?;
                        crate::fsops::mv(backup_path, dst, false)?;
                    }
                    crate::journal::UndoMetadata::MoveWithOverwrite {
                        original_src,
                        backup_path,
                    } => {
                        let dst = entry.dst.as_ref().context("missing dst in journal")?;
                        // 1. Move current dst back to original src (reversing the move)
                        crate::fsops::mv(dst, original_src, false)?;
                        // 2. Restore backup to dst
                        crate::fsops::mv(backup_path, dst, false)?;
                    }
                    crate::journal::UndoMetadata::CopyWithOverwrite {
                        created_dst,
                        backup_path,
                    } => {
                        // 1. Remove the copy at dst
                        if created_dst.is_file() {
                            std::fs::remove_file(created_dst)?;
                        } else if created_dst.is_dir() {
                            std::fs::remove_dir_all(created_dst)?;
                        }
                        // 2. Restore backup to dst
                        // Note: we used created_dst as the path, which should equal entry.dst
                        crate::fsops::mv(backup_path, created_dst, false)?;
                    }
                }
                // Write undo journal entry
                let undo_entry = crate::journal::JournalEntry {
                    id: entry.id,
                    ts: Utc::now(),
                    op: entry.op.clone(),
                    src: entry.src.clone(),
                    dst: entry.dst.clone(),
                    collision: entry.collision.clone(),
                    status: crate::journal::JournalStatus::Undone,
                    undo: None,
                };
                self.write_journal(&undo_entry)?;
            }
        }
        Ok(())
    }
}
