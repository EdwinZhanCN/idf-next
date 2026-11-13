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

fn idf_in(home: &std::path::Path) -> Command {
    let mut c = idf();
    c.env("IDF_NEXT_HOME", home).env("NO_COLOR", "1");
    c
}

#[test]
fn sdk_list_on_empty_cache() {
    let home = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .args(["sdk", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no SDKs in"));
}

#[test]
fn env_needs_a_project() {
    let home = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .args(["-C"])
        .arg(empty.path())
        .arg("env")
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs a project"));
}

#[test]
fn sdk_install_rejects_unsupported_versions() {
    let home = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .args(["sdk", "install", "5.4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ESP-IDF 5.4 is not supported"));
}

#[test]
fn locked_without_lock_fails_before_any_download() {
    let home = tempfile::tempdir().unwrap();
    let proj = tempfile::tempdir().unwrap();
    std::fs::write(
        proj.path().join("idf.toml"),
        "[project]\nname = \"a\"\ntarget = \"esp32\"\nidf = \"6.0\"\n",
    )
    .unwrap();
    idf_in(home.path())
        .arg("-C")
        .arg(proj.path())
        .args(["sdk", "install", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("idf.lock is missing"));
    assert!(!home.path().join("sdk").exists());
}

#[test]
fn sdk_path_for_missing_version() {
    let home = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .args(["sdk", "path", "6.0.3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ESP-IDF 6.0.3 is not installed"));
}
