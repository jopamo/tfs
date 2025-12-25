//! `tfs` - transactional filesystem operation engine.
//!
//! See `README.md` for user documentation, `DESIGN.md` for architecture,
//! and `HACKING.md` for contributor guidelines.

use anyhow::Result;
use clap::Parser;

use tfs::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let exit_code = match cli.command {
        Command::Schema => {
            let schema = tfs::model::generate_schema();
            println!("{}", schema);
            0
        }
        Command::Apply(args) => tfs::engine::apply(args)?,
        Command::Undo(args) => tfs::engine::undo(args)?,
    };
    std::process::exit(exit_code);
}