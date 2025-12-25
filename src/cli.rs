use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Transactional filesystem operation engine.
#[derive(Parser)]
#[command(name = "tfs", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print JSON Schema for manifests.
    Schema,
    /// Validate, preview, or apply a filesystem transaction.
    Apply(ApplyArgs),
    /// Undo a previously applied transaction using its journal.
    Undo(UndoArgs),
}

#[derive(Args)]
pub struct ApplyArgs {
    /// Path to manifest JSON file.
    #[arg(long, required = true)]
    pub manifest: PathBuf,

    /// Only validate manifest, do not execute.
    #[arg(long)]
    pub validate_only: bool,

    /// Simulate execution without writing.
    #[arg(long)]
    pub dry_run: bool,

    /// Output structured JSON to stdout.
    #[arg(long)]
    pub json: bool,

    /// Write journal to specific path.
    #[arg(long)]
    pub journal: Option<PathBuf>,

    /// Override collision policy.
    #[arg(long)]
    pub collision_policy: Option<model::CollisionPolicy>,

    /// Override root directory.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Allow overwrite policies (requires explicit opt-in).
    #[arg(long)]
    pub allow_overwrite: bool,
}

#[derive(Args)]
pub struct UndoArgs {
    /// Path to journal file.
    #[arg(long, required = true)]
    pub journal: PathBuf,

    /// Output structured JSON to stdout.
    #[arg(long)]
    pub json: bool,

    /// Dry-run undo (simulate only).
    #[arg(long)]
    pub dry_run: bool,
}