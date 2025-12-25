use anyhow::{Context, Result};
use chrono::Utc;

/// Transaction manager for `all` or `op` mode.
pub struct TransactionManager {
    mode: crate::model::TransactionMode,
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
            mode,
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
        let src = op.resolved_src.as_ref().map(|p| p.as_path());
        let dst = op.resolved_dst.as_ref().map(|p| p.as_path());

        // Execute based on operation type
        match &op.op {
            crate::model::Operation::Mkdir { dst: dst_path, parents } => {
                let dst = dst.unwrap_or_else(|| dst_path.as_path());
                crate::fsops::mkdir(dst, *parents)?;
                // Record undo metadata
                let undo = crate::journal::UndoMetadata::Mkdir { created_dir: dst.to_path_buf() };
                self.record_success(op.id, src, Some(dst), None, Some(undo))?;
            }
            crate::model::Operation::Move { src: src_path, dst: dst_path, cross_device } => {
                let src = src.unwrap_or_else(|| src_path.as_path());
                let dst = dst.unwrap_or_else(|| dst_path.as_path());
                // Apply collision policy (should have been resolved earlier)
                // TODO: integrate collision policy
                let _result = crate::fsops::mv(src, dst, *cross_device)?;
                let undo = crate::journal::UndoMetadata::Move { original_src: src.to_path_buf() };
                self.record_success(op.id, Some(src), Some(dst), None, Some(undo))?;
            }
            crate::model::Operation::Copy { src: src_path, dst: dst_path, recursive } => {
                let src = src.unwrap_or_else(|| src_path.as_path());
                let dst = dst.unwrap_or_else(|| dst_path.as_path());
                let _result = crate::fsops::cp(src, dst, *recursive)?;
                let undo = crate::journal::UndoMetadata::Copy { created_dst: dst.to_path_buf() };
                self.record_success(op.id, Some(src), Some(dst), None, Some(undo))?;
            }
            crate::model::Operation::Rename { src: src_path, dst: dst_path } => {
                // Alias for move within same directory
                let src = src.unwrap_or_else(|| src_path.as_path());
                let dst = dst.unwrap_or_else(|| dst_path.as_path());
                let _result = crate::fsops::mv(src, dst, false)?;
                let undo = crate::journal::UndoMetadata::Move { original_src: src.to_path_buf() };
                self.record_success(op.id, Some(src), Some(dst), None, Some(undo))?;
            }
            crate::model::Operation::Trash { src: src_path } => {
                let src = src.unwrap_or_else(|| src_path.as_path());
                let result = crate::fsops::trash(src)?;
                let undo = crate::journal::UndoMetadata::Move { original_src: src.to_path_buf() };
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