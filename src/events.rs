use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Structured event emitted during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PlanValidated {
        plan_id: uuid::Uuid,
    },
    OpPlanned {
        op_id: uuid::Uuid,
        op_type: String,
        src: Option<PathBuf>,
        dst: Option<PathBuf>,
    },
    OpStarted {
        op_id: uuid::Uuid,
    },
    OpCompleted {
        op_id: uuid::Uuid,
        bytes_copied: u64,
        final_dst: PathBuf,
    },
    OpFailed {
        op_id: uuid::Uuid,
        error: String,
    },
    TxnCommitted {
        plan_id: uuid::Uuid,
    },
    TxnAborted {
        plan_id: uuid::Uuid,
    },
    UndoStarted {
        journal_id: uuid::Uuid,
    },
    UndoCompleted {
        journal_id: uuid::Uuid,
    },
}
