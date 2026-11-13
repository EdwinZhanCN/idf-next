//! idf-next: a single-binary ESP-IDF toolchain driver.
//!
//! A project directory holds only `idf.toml`, `idf.lock`, `src/` and an
//! optional `components/`. Everything else is generated under `build/` or
//! kept in the global cache under `~/.idf-next/`.

pub mod archive;
pub mod chip;
pub mod cli;
pub mod commands;
pub mod diagnostic;
pub mod fsx;
pub mod home;
pub mod idf_source;
pub mod idf_support;
pub mod lockfile;
pub mod manifest;
pub mod net;
pub mod platform;
pub mod project;
pub mod python;
pub mod sdk;
pub mod tools_json;
pub mod ui;

use clap::CommandFactory;
use cli::{Cli, Command};

/// Run a parsed command line.
pub fn run(cli: Cli) -> miette::Result<()> {
    let g = &cli.global;
    ui::init(match (g.quiet, g.verbose) {
        (true, _) => ui::Verbosity::Quiet,
        (false, 0) => ui::Verbosity::Normal,
        (false, _) => ui::Verbosity::Verbose,
    });
    let ctx = || -> miette::Result<sdk::Ctx> {
        Ok(sdk::Ctx {
            home: home::Home::from_env()?,
            net: net::Net::new(g.offline)?,
        })
    };
    match &cli.command {
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "idf", &mut std::io::stdout());
            Ok(())
        }
        Command::Sdk { action } => commands::sdk::run(&ctx()?, g, action),
        Command::Env { shell } => commands::env::run(&ctx()?, g, *shell),
        Command::Doctor => commands::doctor::run(&home::Home::from_env()?, g),
        other => Err(miette::miette!(
            code = "idf::not_implemented",
            "`idf {}` is not implemented yet (planned for {})",
            other.name(),
            other.phase()
        )),
    }
}
