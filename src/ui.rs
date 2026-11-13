//! Console output: status lines on stderr, progress bars, verbosity.

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

static VERBOSITY: OnceLock<Verbosity> = OnceLock::new();
static MULTI: OnceLock<MultiProgress> = OnceLock::new();

pub fn init(verbosity: Verbosity) {
    let _ = VERBOSITY.set(verbosity);
}

pub fn verbosity() -> Verbosity {
    *VERBOSITY.get().unwrap_or(&Verbosity::Normal)
}

fn multi() -> &'static MultiProgress {
    MULTI.get_or_init(|| {
        let target = if verbosity() == Verbosity::Quiet {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        };
        MultiProgress::with_draw_target(target)
    })
}

fn line(verb: &str, msg: &str) {
    // Cargo-style: right-aligned bold verb, then the message.
    let text = format!("\x1b[1;32m{verb:>12}\x1b[0m {msg}");
    let text = if supports_color() {
        text
    } else {
        format!("{verb:>12} {msg}")
    };
    emit(text);
}

/// Print a line to stderr. On a terminal it goes through the progress
/// manager so bars are not torn; elsewhere (pipes, CI logs) indicatif would
/// drop it, so it is written directly.
fn emit(text: String) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        let _ = multi().println(text);
    } else {
        eprintln!("{text}");
    }
}

fn supports_color() -> bool {
    use std::io::IsTerminal;
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

/// A step that is happening or done, e.g. `status("Installing", "ESP-IDF 6.0.3")`.
pub fn status(verb: &str, msg: impl AsRef<str>) {
    if verbosity() >= Verbosity::Normal {
        line(verb, msg.as_ref());
    }
}

/// Detail shown only with `-v`.
pub fn detail(verb: &str, msg: impl AsRef<str>) {
    if verbosity() >= Verbosity::Verbose {
        line(verb, msg.as_ref());
    }
}

pub fn warn(msg: impl AsRef<str>) {
    let text = if supports_color() {
        format!("\x1b[1;33mwarning\x1b[0m: {}", msg.as_ref())
    } else {
        format!("warning: {}", msg.as_ref())
    };
    emit(text);
}

/// A byte-count progress bar.
pub fn bytes_bar(len: u64, label: impl Into<String>) -> ProgressBar {
    let bar = multi().add(ProgressBar::new(len));
    bar.set_style(
        ProgressStyle::with_template(
            "{prefix:>12.cyan.bold} [{bar:30}] {bytes:>10}/{total_bytes:10} {wide_msg}",
        )
        .expect("valid template")
        .progress_chars("=> "),
    );
    bar.set_prefix("Downloading");
    bar.set_message(label.into());
    bar
}

/// A spinner for steps without a measurable size.
pub fn spinner(label: impl Into<String>) -> ProgressBar {
    let bar = multi().add(ProgressBar::new_spinner());
    bar.set_style(
        ProgressStyle::with_template("{prefix:>12.cyan.bold} {spinner} {wide_msg}")
            .expect("valid template"),
    );
    bar.set_prefix("Working");
    bar.set_message(label.into());
    bar.enable_steady_tick(Duration::from_millis(120));
    bar
}

/// Human-readable byte count.
pub fn bytes(n: u64) -> String {
    indicatif::HumanBytes(n).to_string()
}
