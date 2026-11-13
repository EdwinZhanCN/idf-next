//! idf-next: a single-binary ESP-IDF toolchain driver.
//!
//! A project directory holds only `idf.toml`, `idf.lock`, `src/` and an
//! optional `components/`. Everything else is generated under `build/` or
//! kept in the global cache under `~/.idf-next/`.

pub mod chip;
pub mod cli;
pub mod diagnostic;
pub mod idf_support;
pub mod lockfile;
pub mod manifest;

use clap::CommandFactory;
use cli::{Cli, Command};

/// Run a parsed command line.
pub fn run(cli: Cli) -> miette::Result<()> {
    match cli.command {
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "idf", &mut std::io::stdout());
            Ok(())
        }
        other => Err(miette::miette!(
            code = "idf::not_implemented",
            "`idf {}` is not implemented yet (planned for {})",
            other.name(),
            other.phase()
        )),
    }
}
