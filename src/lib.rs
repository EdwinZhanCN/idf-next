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
pub mod deps;
pub mod diagnostic;
pub mod firmware;
pub mod fsx;
pub mod home;
pub mod idf_source;
pub mod idf_support;
pub mod image;
pub mod kconfig;
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
pub mod user_config;

use clap::CommandFactory;
use cli::{Cli, Command};

/// Run a parsed command line.
pub fn run(mut cli: Cli) -> miette::Result<()> {
    let user = user_config::UserConfig::load()?;
    if cli.global.port.is_none() {
        cli.global.port = user.port.clone();
    }
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
        Command::Config { action } => commands::config::run(&ctx()?, g, action),
        Command::Add {
            specs,
            git,
            rev,
            path,
        } => commands::deps::add(
            &ctx()?,
            g,
            specs,
            git.as_deref(),
            rev.as_deref(),
            path.as_deref(),
        ),
        Command::Remove { names } => commands::deps::remove(&ctx()?, g, names),
        Command::Update { names } => commands::deps::update(&ctx()?, g, names),
        Command::Tree => commands::deps::tree(&ctx()?, g),
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
            serial.baud.or(user.flash_baud),
        ),
        Command::Run { serial } => {
            commands::device::run(&ctx()?, g, serial.baud.or(user.flash_baud))
        }
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
            commands::device::erase(g, what, serial.baud.or(user.flash_baud))
        }
        Command::Image { format, output } => {
            commands::device::image(&ctx()?, g, *format, output.as_deref())
        }
        Command::Size { args } => commands::tools::size(&ctx()?, g, args),
        Command::Efuse { args } => commands::tools::efuse(&ctx()?, g, args),
        Command::Secure { args } => commands::tools::secure(&ctx()?, g, args),
        Command::Coredump { args } => commands::tools::coredump(&ctx()?, g, args),
        Command::Debug { args } => commands::tools::debug(&ctx()?, g, args),
    }
}
