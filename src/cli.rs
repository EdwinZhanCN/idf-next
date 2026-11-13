//! Command-line interface: the revised command tree from the plan.

use crate::chip::Chip;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "idf",
    bin_name = "idf",
    about = "ESP-IDF projects from a single idf.toml",
    version = env!("CARGO_PKG_VERSION"),
    long_version = crate::idf_support::LONG_VERSION,
    help_template = HELP_TEMPLATE,
    subcommand_help_heading = "Commands",
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

// Groups the top-level commands the way the plan's command tree does. Clap
// only lists commands flat, so the overview is written out here and must be
// kept in sync with `Command` (a test checks every command appears).
const HELP_TEMPLATE: &str = "\
{about-with-newline}
{usage-heading} {usage}

Project:
  new          Create a project in a new directory
  init         Create a project in the current directory
  add          Add dependencies
  remove       Remove dependencies
  update       Update idf.lock within constraints
  tree         Show dependency tree
  target       Set or list chip targets
  config       menu | get | set | diff | explain

Build & device:
  build        Build firmware
  run          Build, flash, monitor
  flash        Flash firmware
  monitor      Serial monitor with backtrace decoding
  erase        Erase flash, a partition, or a region
  image        Export merged bin, UF2, DFU
  size         Size analysis
  clean        Clean build artifacts

Debug & security:
  debug        Launch OpenOCD + GDB
  coredump     Decode a core dump
  efuse        Read, summarize, burn eFuses
  secure       Keys, signing, flash encryption

Environment:
  sdk          install | list | use | remove | path
  env          Show environment
  doctor       Diagnose environment
  native       Run upstream idf.py in the managed env
  completions  Generate shell completions

{options}{after-help}";

#[derive(Debug, Args)]
pub struct GlobalArgs {
    /// Run as if started in PATH
    #[arg(short = 'C', long = "project", value_name = "PATH", global = true)]
    pub project: Option<PathBuf>,

    /// Chip for this invocation only (overrides idf.toml)
    #[arg(short, long, value_name = "CHIP", global = true)]
    pub target: Option<Chip>,

    /// Serial port (default: auto-detect by USB VID/PID)
    #[arg(short, long, value_name = "PORT", env = "IDF_PORT", global = true)]
    pub port: Option<String>,

    /// Use the release profile
    #[arg(short, long, global = true, conflicts_with = "profile")]
    pub release: bool,

    /// Use the named profile
    #[arg(long, value_name = "NAME", global = true)]
    pub profile: Option<String>,

    /// Never touch the network
    #[arg(long, global = true)]
    pub offline: bool,

    /// Fail instead of updating idf.lock
    #[arg(long, global = true)]
    pub locked: bool,

    /// Never prompt
    #[arg(long, global = true)]
    pub no_input: bool,

    /// More output (repeatable)
    #[arg(short, long, action = ArgAction::Count, global = true, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Less output
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

impl GlobalArgs {
    /// Selected profile name: `--profile`, else `release` for `-r`, else `dev`.
    pub fn profile_name(&self) -> &str {
        match (&self.profile, self.release) {
            (Some(p), _) => p,
            (None, true) => "release",
            (None, false) => "dev",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    // ---- Project
    /// Create a project in a new directory
    New { path: PathBuf },
    /// Create a project in the current directory
    Init,
    /// Add dependencies
    Add {
        /// `namespace/name[@requirement]`, or a name with --git / --path
        #[arg(required = true)]
        specs: Vec<String>,
        #[arg(long, conflicts_with = "path")]
        git: Option<String>,
        #[arg(long, requires = "git")]
        rev: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Remove dependencies
    Remove {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Update idf.lock within the constraints in idf.toml
    Update {
        /// Only these dependencies (default: all)
        names: Vec<String>,
    },
    /// Show dependency tree
    Tree,
    /// Set or list chip targets
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },
    /// Read and change Kconfig values
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    // ---- Build & device
    /// Build firmware
    Build,
    /// Build, flash, monitor
    Run {
        #[command(flatten)]
        serial: SerialArgs,
    },
    /// Flash firmware
    Flash {
        /// Only the application
        #[arg(long)]
        app: bool,
        /// Only the bootloader
        #[arg(long)]
        bootloader: bool,
        /// Only the partition table
        #[arg(long)]
        partition_table: bool,
        #[command(flatten)]
        serial: SerialArgs,
    },
    /// Serial monitor with backtrace decoding
    Monitor {
        #[command(flatten)]
        serial: SerialArgs,
    },
    /// Erase flash, a partition, or a region
    Erase {
        /// Erase one partition by name
        #[arg(long, value_name = "NAME", conflicts_with = "region")]
        partition: Option<String>,
        /// Erase OFFSET and SIZE bytes
        #[arg(long, num_args = 2, value_names = ["OFFSET", "SIZE"])]
        region: Option<Vec<String>>,
    },
    /// Export merged bin, UF2, DFU
    Image {
        #[arg(long, value_enum, default_value_t = ImageFormat::Bin)]
        format: ImageFormat,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Size analysis
    Size {
        /// Arguments passed to esp-idf-size
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Clean build artifacts
    Clean {
        /// Also remove this project's entries from the global cache
        #[arg(long)]
        all: bool,
    },

    // ---- Debug & security
    /// Launch OpenOCD + GDB
    Debug,
    /// Decode a core dump
    Coredump {
        /// Arguments passed to esp-coredump
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Read, summarize, burn eFuses
    Efuse {
        /// Arguments passed to espefuse
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Keys, signing, flash encryption
    Secure {
        /// Arguments passed to espsecure
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ---- Environment
    /// Install and select ESP-IDF SDKs
    Sdk {
        #[command(subcommand)]
        action: SdkAction,
    },
    /// Show environment
    Env {
        /// Print shell exports
        #[arg(long)]
        shell: bool,
    },
    /// Diagnose environment
    Doctor,
    /// Run upstream idf.py in the managed env
    Native {
        /// Arguments passed to idf.py
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Args)]
pub struct SerialArgs {
    /// Baud rate
    #[arg(short, long, value_name = "BAUD")]
    pub baud: Option<u32>,
}

#[derive(Debug, Subcommand)]
pub enum TargetAction {
    /// Write the target into idf.toml
    Set { chip: Chip },
    /// List known chips
    List,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Interactive menuconfig; changes are written back to idf.toml
    Menu,
    /// Print the effective value of a symbol
    Get { key: String },
    /// Set symbols in idf.toml
    Set {
        /// `KEY=VALUE` pairs
        #[arg(required = true)]
        assignments: Vec<String>,
    },
    /// Show values that differ from the defaults
    Diff,
    /// Explain a symbol: help text, dependencies, where its value comes from
    Explain { key: String },
}

#[derive(Debug, Subcommand)]
pub enum SdkAction {
    /// Install an SDK with its toolchain and venv
    Install {
        /// e.g. 6.0 or 6.0.1 (default: what the project asks for)
        version: Option<String>,
    },
    /// List installed SDKs
    List,
    /// Pin the project to an SDK version
    Use { version: String },
    /// Remove an installed SDK
    Remove { version: String },
    /// Print an SDK's install path
    Path { version: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImageFormat {
    Bin,
    Uf2,
    Dfu,
}

impl Command {
    /// The command's name as typed (`idf <name>`).
    pub fn name(&self) -> &'static str {
        match self {
            Command::New { .. } => "new",
            Command::Init => "init",
            Command::Add { .. } => "add",
            Command::Remove { .. } => "remove",
            Command::Update { .. } => "update",
            Command::Tree => "tree",
            Command::Target { .. } => "target",
            Command::Config { .. } => "config",
            Command::Build => "build",
            Command::Run { .. } => "run",
            Command::Flash { .. } => "flash",
            Command::Monitor { .. } => "monitor",
            Command::Erase { .. } => "erase",
            Command::Image { .. } => "image",
            Command::Size { .. } => "size",
            Command::Clean { .. } => "clean",
            Command::Debug => "debug",
            Command::Coredump { .. } => "coredump",
            Command::Efuse { .. } => "efuse",
            Command::Secure { .. } => "secure",
            Command::Sdk { .. } => "sdk",
            Command::Env { .. } => "env",
            Command::Doctor => "doctor",
            Command::Native { .. } => "native",
            Command::Completions { .. } => "completions",
        }
    }

    /// The plan phase that delivers this command.
    pub fn phase(&self) -> &'static str {
        match self {
            Command::Completions { .. } => "P0",
            Command::Sdk { .. } | Command::Env { .. } | Command::Doctor => "P1",
            Command::New { .. }
            | Command::Init
            | Command::Target { .. }
            | Command::Build
            | Command::Run { .. }
            | Command::Flash { .. }
            | Command::Monitor { .. }
            | Command::Erase { .. }
            | Command::Image { .. }
            | Command::Clean { .. }
            | Command::Native { .. } => "P2",
            Command::Config { .. } => "P3",
            Command::Add { .. }
            | Command::Remove { .. }
            | Command::Update { .. }
            | Command::Tree => "P4",
            Command::Size { .. }
            | Command::Debug
            | Command::Coredump { .. }
            | Command::Efuse { .. }
            | Command::Secure { .. } => "P5",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn help_overview_lists_every_command() {
        let cmd = Cli::command();
        for sub in cmd.get_subcommands() {
            let name = sub.get_name();
            assert!(
                HELP_TEMPLATE
                    .lines()
                    .any(|l| l.trim_start().starts_with(&format!("{name} "))),
                "`{name}` missing from HELP_TEMPLATE"
            );
        }
    }

    #[test]
    fn profile_selection() {
        let p =
            |args: &[&str]| Cli::try_parse_from(args).map(|c| c.global.profile_name().to_owned());
        assert_eq!(p(&["idf", "build"]).unwrap(), "dev");
        assert_eq!(p(&["idf", "build", "-r"]).unwrap(), "release");
        assert_eq!(
            p(&["idf", "-C", ".", "build", "--profile", "ci"]).unwrap(),
            "ci"
        );
        assert!(p(&["idf", "build", "-r", "--profile", "ci"]).is_err());
    }

    #[test]
    fn rejects_bad_target() {
        let err = Cli::try_parse_from(["idf", "build", "-t", "ESP32S3"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("did you mean \"esp32s3\""), "{err}");
    }

    #[test]
    fn passthrough_keeps_flags() {
        let cli = Cli::try_parse_from(["idf", "efuse", "summary", "--format", "json"]).unwrap();
        let Command::Efuse { args } = cli.command else {
            panic!()
        };
        assert_eq!(args, ["summary", "--format", "json"]);
    }
}
