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

#[test]
fn new_creates_a_minimal_project() {
    let tmp = tempfile::tempdir().unwrap();
    idf_in(tmp.path())
        .current_dir(tmp.path())
        .args(["-t", "esp32c3", "new", "blink"])
        .assert()
        .success();
    let dir = tmp.path().join("blink");
    let toml = std::fs::read_to_string(dir.join("idf.toml")).unwrap();
    assert!(toml.contains("name = \"blink\"") && toml.contains("target = \"esp32c3\""));
    assert!(dir.join("src/main.c").is_file());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, [".gitignore", "idf.toml", "src"]);
    // Never over an existing directory.
    idf_in(tmp.path())
        .current_dir(tmp.path())
        .args(["new", "blink"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn init_keeps_existing_sources_and_refuses_twice() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("app");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/app.c"), "void app_main(void) {}\n").unwrap();
    idf_in(tmp.path())
        .arg("-C")
        .arg(&dir)
        .arg("init")
        .assert()
        .success();
    assert!(!dir.join("src/main.c").exists());
    idf_in(tmp.path())
        .arg("-C")
        .arg(&dir)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("has an idf.toml"));
    // Directory names that are not project names are refused.
    let bad = tmp.path().join("1bad");
    std::fs::create_dir(&bad).unwrap();
    idf_in(tmp.path())
        .arg("-C")
        .arg(&bad)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used as a project name"));
}

#[test]
fn target_set_keeps_formatting() {
    let tmp = tempfile::tempdir().unwrap();
    let toml = "# mine\n[project]\nname = \"x\"  # keep\ntarget = \"esp32\"\nidf = \"6.0\"\n";
    std::fs::write(tmp.path().join("idf.toml"), toml).unwrap();
    idf_in(tmp.path())
        .arg("-C")
        .arg(tmp.path())
        .args(["target", "set", "esp32c6"])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("idf.toml")).unwrap(),
        toml.replace("\"esp32\"", "\"esp32c6\"")
    );
    idf_in(tmp.path())
        .arg("-C")
        .arg(tmp.path())
        .args(["target", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("esp32c6  (this project)"));
}

#[test]
fn clean_removes_one_profile_or_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("idf.toml"),
        "[project]\nname = \"x\"\ntarget = \"esp32\"\nidf = \"6.0\"\n",
    )
    .unwrap();
    for p in ["dev", "release"] {
        std::fs::create_dir_all(root.join("build/esp32").join(p)).unwrap();
    }
    std::os::unix::fs::symlink(
        "build/esp32/dev/compile_commands.json",
        root.join("compile_commands.json"),
    )
    .unwrap();
    idf_in(root)
        .arg("-C")
        .arg(root)
        .arg("clean")
        .assert()
        .success();
    assert!(!root.join("build/esp32/dev").exists());
    assert!(root.join("build/esp32/release").exists());
    // The link pointed into the removed build.
    assert!(std::fs::symlink_metadata(root.join("compile_commands.json")).is_err());
    idf_in(root)
        .arg("-C")
        .arg(root)
        .args(["clean", "--all"])
        .assert()
        .success();
    assert!(!root.join("build").exists());
}

#[test]
fn erase_checks_the_region_before_connecting() {
    let home = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .current_dir(empty.path())
        .env("IDF_PORT", "/dev/idf-next-no-such-port")
        .args(["erase", "--region", "0x9001", "4K"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiples of 4K"));
}

#[test]
fn monitor_names_the_port_it_cannot_open() {
    let home = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    idf_in(home.path())
        .current_dir(empty.path())
        .args(["monitor", "-p", "/dev/idf-next-no-such-port"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "opening /dev/idf-next-no-such-port",
        ));
}
