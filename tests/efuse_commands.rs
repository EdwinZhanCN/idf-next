//! The eFuse safety gate keeps a list of `espefuse` commands that burn. This
//! reads the list of commands from the installed `espefuse` itself (it groups
//! them as "Burn commands" and "Read commands" in `--help`), so a command added
//! upstream cannot slip past the gate unnoticed.
//!
//! Needs an installed SDK (`idf sdk install`); skipped without one.

use idf_next::commands::tools::{EFUSE_BURN, EFUSE_READ_ONLY};
use std::path::PathBuf;
use std::process::Command;

const CHIPS: &[&str] = &[
    "esp32", "esp32s2", "esp32s3", "esp32c2", "esp32c3", "esp32c5", "esp32c6", "esp32c61",
    "esp32h2", "esp32h21", "esp32h4", "esp32p4",
];

/// `(sdk dir, venv python)` of any installed SDK.
fn installed() -> Option<(PathBuf, PathBuf)> {
    let home = std::env::var_os("IDF_NEXT_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".idf-next")))?;
    for sdk in std::fs::read_dir(home.join("sdk")).ok()?.flatten() {
        let py_root = sdk.path().join("python");
        for env in std::fs::read_dir(py_root).ok()?.flatten() {
            let python = env.path().join("venv/bin/python");
            if python.is_file() && sdk.path().join("esp-idf").is_dir() {
                return Some((sdk.path().join("esp-idf"), python));
            }
        }
    }
    None
}

/// Commands listed under each help panel: `(panel, [command])`.
fn panels(help: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in help.lines() {
        if let Some(rest) = line.strip_prefix("╭─ ") {
            let name = rest.split(" ─").next().unwrap_or("").trim().to_owned();
            out.push((name, Vec::new()));
            current = Some(out.len() - 1);
        } else if line.starts_with('╰') {
            current = None;
        } else if let (Some(i), Some(body)) = (current, line.strip_prefix("│ ")) {
            // Continuation lines of a description start with spaces.
            if body.starts_with(' ') {
                continue;
            }
            if let Some(word) = body.split_whitespace().next() {
                out[i].1.push(word.to_owned());
            }
        }
    }
    out
}

#[test]
fn every_upstream_command_is_classified() {
    let Some((idf_path, python)) = installed() else {
        eprintln!("skipped: no SDK installed");
        return;
    };
    for chip in CHIPS {
        let out = Command::new(&python)
            .args(["-m", "espefuse", "--virt", "--chip", chip, "--help"])
            .env("IDF_PATH", &idf_path)
            .env("COLUMNS", "200")
            .env("NO_COLOR", "1")
            .output()
            .expect("running espefuse");
        let help = String::from_utf8_lossy(&out.stdout);
        let panels = panels(&help);
        let burn: Vec<&String> = panels
            .iter()
            .filter(|(n, _)| n == "Burn commands")
            .flat_map(|(_, c)| c)
            .collect();
        let read: Vec<&String> = panels
            .iter()
            .filter(|(n, _)| n == "Read commands")
            .flat_map(|(_, c)| c)
            .collect();
        assert!(
            !burn.is_empty() && !read.is_empty(),
            "{chip}: no command panels in\n{help}"
        );
        for c in &burn {
            assert!(
                EFUSE_BURN.contains(&c.as_str()),
                "{chip}: espefuse burns with `{c}`, which the gate does not know"
            );
        }
        for c in &read {
            assert!(
                EFUSE_READ_ONLY.contains(&c.as_str()),
                "{chip}: espefuse lists `{c}` as read-only, which the gate does not know"
            );
        }
        // Any other panel with commands would be unclassified.
        for (name, cmds) in &panels {
            let known = matches!(name.as_str(), "Burn commands" | "Read commands" | "Options");
            assert!(
                known || cmds.is_empty(),
                "{chip}: unclassified panel {name:?}: {cmds:?}"
            );
        }
    }
}

#[test]
fn panel_parser_reads_commands() {
    let help = "╭─ Burn commands ─╮\n│ burn-efuse    Burn it.   │\n│               more text  │\n│ burn-bit      Burn.      │\n╰──╯\n";
    assert_eq!(
        panels(help),
        [(
            "Burn commands".to_owned(),
            vec!["burn-efuse".to_owned(), "burn-bit".to_owned()]
        )]
    );
}
