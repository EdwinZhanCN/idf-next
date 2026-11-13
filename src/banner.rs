//! The logo on `idf`, `idf --help` and `idf --version`.
//!
//! Shown only for the top-level command and only on a terminal (see
//! [`logo::Terminal::detect`]); piped or scripted output is exactly clap's.

use crate::cli::Cli;
use crate::idf_support::SUPPORTED_IDF_RANGE;
use crate::logo::{self, ColorMode, Terminal};
use clap::CommandFactory;
use clap::error::ErrorKind;
use std::ffi::OsString;
use std::io::Write;

/// Exit for a clap error, drawing the logo first when it applies.
pub fn exit(err: clap::Error) -> ! {
    let shown = match err.kind() {
        ErrorKind::DisplayVersion => version(&err),
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => help(&err),
        _ => false,
    };
    if shown {
        std::process::exit(err.exit_code());
    }
    err.exit()
}

fn version(err: &clap::Error) -> bool {
    if !wanted(std::env::args_os().skip(1)) {
        return false;
    }
    let Some(term) = Terminal::detect() else {
        return false;
    };
    let text = err.render().to_string();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if let Some(first) = lines.first_mut() {
        *first = bold(&term, first);
    }
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let Some(cols) = term.pick(width, term.rows.saturating_sub(2)) else {
        return false;
    };
    print(format!("\n{}\n", logo::compose(&term, cols, &lines)))
}

fn help(err: &clap::Error) -> bool {
    if !wanted(std::env::args_os().skip(1)) {
        return false;
    }
    let Some(term) = Terminal::detect() else {
        return false;
    };
    let cmd = Cli::command();
    let about = cmd.get_about().map(|a| a.to_string()).unwrap_or_default();
    let side = [
        bold(&term, &format!("idf {}", env!("CARGO_PKG_VERSION"))),
        about.clone(),
        dim(&term, &format!("ESP-IDF {SUPPORTED_IDF_RANGE}")),
    ];
    let width = side.iter().map(|l| visible_len(l)).max().unwrap_or(0);
    // At most half the screen, so the help itself stays in view.
    let Some(cols) = term.pick(width, term.rows / 2) else {
        return false;
    };
    let help = err.render().ansi().to_string();
    // The logo's side text already carries the about line.
    let rest = help
        .strip_prefix(about.as_str())
        .map_or(help.as_str(), |r| r.trim_start_matches('\n'));
    print(format!("\n{}\n{rest}", logo::compose(&term, cols, &side)))
}

fn print(text: String) -> bool {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .is_ok()
}

fn bold(term: &Terminal, s: &str) -> String {
    match term.color {
        ColorMode::None => s.to_owned(),
        _ => format!("\x1b[1m{s}\x1b[0m"),
    }
}

fn dim(term: &Terminal, s: &str) -> String {
    match term.color {
        ColorMode::None => s.to_owned(),
        _ => format!("\x1b[2m{s}\x1b[0m"),
    }
}

fn visible_len(s: &str) -> usize {
    let mut n = 0;
    let mut esc = false;
    for c in s.chars() {
        match (esc, c) {
            (false, '\x1b') => esc = true,
            (true, 'm') => esc = false,
            (true, _) => {}
            (false, _) => n += 1,
        }
    }
    n
}

/// Whether the arguments ask for the top-level help or version (not a
/// subcommand's help) without `--quiet`.
fn wanted(args: impl Iterator<Item = OsString>) -> bool {
    let cmd = Cli::command();
    let takes_value = |short: Option<char>, long: Option<&str>| {
        cmd.get_arguments().any(|a| {
            a.get_action().takes_values()
                && (short.is_some() && a.get_short() == short
                    || long.is_some() && a.get_long() == long)
        })
    };
    let is_quiet =
        |short: Option<char>, long: Option<&str>| short == Some('q') || long == Some("quiet");
    let mut args = args.map(|a| a.to_string_lossy().into_owned());
    let mut help_cmd = false;
    while let Some(arg) = args.next() {
        if arg == "--" {
            return false;
        } else if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = long
                .split_once('=')
                .map_or((long, false), |(n, _)| (n, true));
            if is_quiet(None, Some(name)) {
                return false;
            }
            if !inline && takes_value(None, Some(name)) {
                args.next();
            }
        } else if let Some(shorts) = arg.strip_prefix('-').filter(|s| !s.is_empty()) {
            // A cluster such as `-vq`; a value-taking flag swallows the rest
            // of the cluster, or the next argument if it ends the cluster.
            for (i, c) in shorts.char_indices() {
                if is_quiet(Some(c), None) {
                    return false;
                }
                if takes_value(Some(c), None) {
                    if i + c.len_utf8() == shorts.len() {
                        args.next();
                    }
                    break;
                }
            }
        } else if arg == "help" && !help_cmd {
            help_cmd = true;
        } else {
            return false; // a subcommand
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(args: &str) -> bool {
        wanted(args.split_whitespace().map(OsString::from))
    }

    #[test]
    fn top_level_help_and_version() {
        assert!(top(""));
        assert!(top("--help"));
        assert!(top("-h"));
        assert!(top("--version"));
        assert!(top("help"));
        assert!(top("-C some/dir --help"));
        assert!(top("-Cdir -t esp32 -v --help"));
        assert!(top("--project=x -p /dev/tty.usb --help"));
    }

    #[test]
    fn not_for_subcommands_or_quiet() {
        assert!(!top("sdk --help"));
        assert!(!top("help sdk"));
        assert!(!top("-C dir build -h"));
        assert!(!top("-q --version"));
        assert!(!top("--quiet --help"));
        assert!(!top("-vq"));
    }
}
