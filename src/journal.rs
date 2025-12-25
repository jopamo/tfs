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
    /// Undo a move that involved an overwrite: move back to src, then restore backup to dst.
    MoveWithOverwrite {
        original_src: PathBuf,
        backup_path: PathBuf,
    },
    /// Undo a copy that involved an overwrite: remove dst, then restore backup to dst.
    CopyWithOverwrite {
        created_dst: PathBuf,
        backup_path: PathBuf,
    },
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
        .map(|line| {
            serde_json::from_str(line).map_err(|e| anyhow::anyhow!("invalid journal line: {}", e))
        })
        .collect::<anyhow::Result<_>>()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let journal_path = dir.path().join("journal.jsonl");

        let mut writer = JournalWriter::open(journal_path.clone()).unwrap();

        let id1 = Uuid::new_v4();
        let entry1 = JournalEntry {
            id: id1,
            ts: Utc::now(),
            op: "op1".to_string(),
            src: None,
            dst: None,
            collision: None,
            status: JournalStatus::Start,
            undo: None,
        };

        writer.write(&entry1).unwrap();

        let id2 = Uuid::new_v4();
        let entry2 = JournalEntry {
            id: id2,
            ts: Utc::now(),
            op: "op2".to_string(),
            src: Some(PathBuf::from("src")),
            dst: Some(PathBuf::from("dst")),
            collision: None,
            status: JournalStatus::Ok,
            undo: Some(UndoMetadata::Move {
                original_src: PathBuf::from("orig"),
            }),
        };

        writer.write(&entry2).unwrap();

        // Read back
        let entries = read_journal(journal_path).unwrap();
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].id, id1);
        assert_eq!(entries[0].status, JournalStatus::Start);

        assert_eq!(entries[1].id, id2);
        assert_eq!(entries[1].status, JournalStatus::Ok);
        if let Some(UndoMetadata::Move { original_src }) = &entries[1].undo {
            assert_eq!(original_src, &PathBuf::from("orig"));
        } else {
            panic!("Wrong undo metadata");
        }
    }
}
