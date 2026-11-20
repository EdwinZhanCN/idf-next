//! idf-next: a single-binary ESP-IDF toolchain driver.
//!
//! A project directory holds only `idf.toml`, `idf.lock`, `src/` and an
//! optional `components/`. Everything else is generated under `build/` or
//! kept in the global cache under `~/.idf-next/`.

pub mod archive;
pub mod banner;
pub mod build;
pub mod chip;
pub mod cli;
pub mod commands;
pub mod diagnostic;
pub mod firmware;
pub mod fsx;
pub mod home;
pub mod idf_source;
pub mod idf_support;
pub mod image;
pub mod lockfile;
pub mod logo;
pub mod manifest;
pub mod monitor;
pub mod net;
pub mod platform;
pub mod project;
pub mod python;
pub mod sdk;
pub mod serial;
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
    ui::install_logger();
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
        Command::New { path } => commands::new::new(g, path),
        Command::Init => commands::new::init(g),
        Command::Target { action } => commands::new::target(g, action),
        Command::Build => commands::build::run_build(&ctx()?, g),
        Command::Clean { all } => commands::build::clean(g, *all),
        Command::Native { args } => commands::build::native(&ctx()?, g, args),
        Command::Flash {
            app,
            bootloader,
            partition_table,
            serial,
        } => commands::device::flash(
            &ctx()?,
            g,
            commands::device::Selection {
                app: *app,
                bootloader: *bootloader,
                partition_table: *partition_table,
            },
            serial.baud,
        ),
        Command::Run { serial } => commands::device::run(&ctx()?, g, serial.baud),
        Command::Monitor { no_reset, serial } => {
            commands::device::monitor(g, serial.baud, *no_reset)
        }
        Command::Erase {
            partition,
            region,
            serial,
        } => {
            use commands::device::EraseWhat;
            let what = match (partition, region.as_deref()) {
                (Some(name), _) => EraseWhat::Partition(name.clone()),
                (None, Some([offset, size])) => EraseWhat::Region(*offset, *size),
                _ => EraseWhat::All,
            };
            commands::device::erase(g, what, serial.baud)
        }
        Command::Image { format, output } => {
            commands::device::image(&ctx()?, g, *format, output.as_deref())
        }
        other => Err(miette::miette!(
            code = "idf::not_implemented",
            "`idf {}` is not implemented yet (planned for {})",
            other.name(),
            other.phase()
        )),
    }
}
