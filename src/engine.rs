use anyhow::{Context, Result};
use crate::cli::{ApplyArgs, UndoArgs};
use crate::exit_codes::exit;
use crate::journal::JournalWriter;
use crate::model;
use crate::reporter::Reporter;
use crate::resolve;
use crate::transaction::TransactionManager;
use crate::validate;

pub fn apply(args: ApplyArgs) -> Result<i32> {
    let mut reporter = Reporter::new(args.json);

    // Load and validate plan
    let mut plan = model::load_plan(&args.manifest)
        .context("failed to load manifest")?;
    if let Some(root) = args.root {
        plan.root = root;
    }
    if let Some(collision_policy) = args.collision_policy {
        plan.collision_policy = collision_policy;
    }
    plan.allow_overwrite = args.allow_overwrite;
    plan.validate()?;
    resolve::validate_root_confinement(&plan)?;

    // Normalize operations
    let normalized = validate::normalize_plan(&plan)?;

    // Preflight checks
    validate::preflight_check(&plan)?;

    if args.validate_only {
        reporter.record(crate::events::Event::PlanValidated { plan_id: uuid::Uuid::new_v4() });
        return Ok(exit::SUCCESS);
    }

    // Open journal if needed
    let journal_writer = if let Some(journal_path) = args.journal {
        Some(JournalWriter::open(journal_path)?)
    } else {
        None
    };

    let mut txn = TransactionManager::new(
        plan.transaction,
        plan.collision_policy,
        plan.allow_overwrite,
        journal_writer,
    );

    if args.dry_run {
        // Simulate each operation without writing
        for op in &normalized {
            reporter.record(crate::events::Event::OpPlanned {
                op_id: op.id,
                op_type: format!("{:?}", op.op),
                src: op.resolved_src.clone(),
                dst: op.resolved_dst.clone(),
            });
        }
        reporter.record(crate::events::Event::TxnCommitted { plan_id: uuid::Uuid::new_v4() });
        return Ok(exit::SUCCESS);
    }

    // Real execution
    for op in &normalized {
        reporter.record(crate::events::Event::OpStarted { op_id: op.id });
        match txn.execute(op) {
            Ok(()) => {
                reporter.record(crate::events::Event::OpCompleted {
                    op_id: op.id,
                    bytes_copied: 0, // TODO: fill from result
                    final_dst: op.resolved_dst.clone().unwrap_or_default(),
                });
            }
            Err(e) => {
                reporter.record(crate::events::Event::OpFailed {
                    op_id: op.id,
                    error: e.to_string(),
                });
                if plan.transaction == model::TransactionMode::All {
                    txn.rollback()?;
                    reporter.record(crate::events::Event::TxnAborted { plan_id: uuid::Uuid::new_v4() });
                    return Ok(exit::TRANSACTIONAL_FAILURE);
                }
                // In op mode, continue with next operation
            }
        }
    }

    txn.commit()?;
    reporter.record(crate::events::Event::TxnCommitted { plan_id: uuid::Uuid::new_v4() });
    Ok(exit::SUCCESS)
}

pub fn undo(args: UndoArgs) -> Result<i32> {
    let mut reporter = Reporter::new(args.json);
    let journal_path = args.journal.clone();
    let entries = crate::journal::read_journal(journal_path.clone())?;
    reporter.record(crate::events::Event::UndoStarted { journal_id: uuid::Uuid::new_v4() });

    // Open journal for appending undo records
    let mut journal_writer = crate::journal::JournalWriter::open(journal_path)?;

    if args.dry_run {
        // Simulate undo
        for entry in entries.iter().rev() {
            if entry.status == crate::journal::JournalStatus::Ok {
                // Would undo
            }
        }
        reporter.record(crate::events::Event::UndoCompleted { journal_id: uuid::Uuid::new_v4() });
        return Ok(exit::SUCCESS);
    }

    // Real undo
    for entry in entries.iter().rev() {
        if entry.status != crate::journal::JournalStatus::Ok {
            continue; // skip already undone or failed operations
        }
        if let Some(undo) = &entry.undo {
            match undo {
                crate::journal::UndoMetadata::Move { original_src } => {
                    let dst = entry.dst.as_ref().context("missing dst in journal")?;
                    crate::fsops::mv(dst, original_src, false)?;
                }
                crate::journal::UndoMetadata::Copy { created_dst } => {
                    if created_dst.is_file() {
                        std::fs::remove_file(created_dst)?;
                    } else if created_dst.is_dir() {
                        std::fs::remove_dir_all(created_dst)?;
                    }
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
                ts: chrono::Utc::now(),
                op: entry.op.clone(),
                src: entry.src.clone(),
                dst: entry.dst.clone(),
                collision: entry.collision.clone(),
                status: crate::journal::JournalStatus::Undone,
                undo: None,
            };
            journal_writer.write(&undo_entry)?;
        }
    }
    reporter.record(crate::events::Event::UndoCompleted { journal_id: uuid::Uuid::new_v4() });
    Ok(exit::SUCCESS)
}