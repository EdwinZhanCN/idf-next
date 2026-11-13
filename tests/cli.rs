use assert_cmd::Command;
use predicates::prelude::*;

fn idf() -> Command {
    Command::cargo_bin("idf").unwrap()
}

#[test]
fn version_reports_tool_and_idf_range() {
    idf().arg("--version").assert().success().stdout(format!(
        "idf {}\nsupported ESP-IDF: {}\n",
        env!("CARGO_PKG_VERSION"),
        idf_next::idf_support::SUPPORTED_IDF_RANGE
    ));
    // -V stays on one line for scripts.
    idf()
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("idf {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_shows_grouped_commands() {
    idf()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Build & device:"))
        .stdout(predicate::str::contains("--offline"));
}

#[test]
fn unimplemented_commands_say_when() {
    idf()
        .args(["build"])
        .env("NO_COLOR", "1")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`idf build` is not implemented yet (planned for P2)",
        ));
}

#[test]
fn completions_generate() {
    idf()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef idf"));
}
