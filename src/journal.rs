use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Journal entry status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JournalStatus {
    Start,
    Ok,
    Fail,
    Undone,
}

/// A single journal entry (NDJSON line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Unique operation ID.
    pub id: Uuid,
    /// Monotonic timestamp (ISO 8601).
    pub ts: DateTime<Utc>,
    /// Operation type.
    pub op: String,
    /// Resolved source path (if applicable).
    pub src: Option<PathBuf>,
    /// Resolved destination path (if applicable).
    pub dst: Option<PathBuf>,
    /// Collision resolution details.
    pub collision: Option<CollisionDetails>,
    /// Status transition.
    pub status: JournalStatus,
    /// Undo metadata.
    pub undo: Option<UndoMetadata>,
}

/// Details about collision resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionDetails {
    /// Policy used.
    pub policy: crate::model::CollisionPolicy,
    /// Final destination path (may differ from original dst).
    pub final_dst: PathBuf,
    /// Backup path if overwritten.
    pub backup_path: Option<PathBuf>,
}

/// Metadata needed to undo an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UndoMetadata {
    /// Undo a move: move back to original location.
    Move { original_src: PathBuf },
    /// Undo a copy: remove created destination.
    Copy { created_dst: PathBuf },
    /// Undo a mkdir: remove directory (if empty).
    Mkdir { created_dir: PathBuf },
    /// Undo an overwrite_with_backup: restore backup.
    Overwrite { backup_path: PathBuf },
}

/// Journal writer that appends NDJSON lines.
pub struct JournalWriter {
    file: std::fs::File,
}

impl JournalWriter {
    /// Open journal file for appending.
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file })
    }

    /// Write a journal entry.
    pub fn write(&mut self, entry: &JournalEntry) -> anyhow::Result<()> {
        let line = serde_json::to_string(entry)?;
        use std::io::Write;
        writeln!(&mut self.file, "{}", line)?;
        self.file.sync_all()?;
        Ok(())
    }
}

/// Read journal entries from a file.
pub fn read_journal(path: PathBuf) -> anyhow::Result<Vec<JournalEntry>> {
    let content = std::fs::read_to_string(path)?;
    let entries: Vec<JournalEntry> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| anyhow::anyhow!("invalid journal line: {}", e)))
        .collect::<anyhow::Result<_>>()?;
    Ok(entries)
}