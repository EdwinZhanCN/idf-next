//! `idf doctor`: read-only checks of the tool, the project and its SDK.

use crate::cli::GlobalArgs;
use crate::home::Home;
use crate::idf_support;
use crate::platform;
use crate::project::Project;
use crate::python::{self, Uv};
use crate::sdk::{self, Installed, Sdk};
use crate::tools_json::Selected;
use miette::{Result, bail};
use std::process::Command;

#[derive(Default)]
struct Report {
    failures: usize,
    warnings: usize,
}

impl Report {
    fn ok(&mut self, msg: impl AsRef<str>) {
        println!("  \u{2713} {}", msg.as_ref());
    }

    fn warn(&mut self, msg: impl AsRef<str>, hint: impl AsRef<str>) {
        self.warnings += 1;
        println!("  ! {}\n      {}", msg.as_ref(), hint.as_ref());
    }

    fn fail(&mut self, msg: impl AsRef<str>, hint: impl AsRef<str>) {
        self.failures += 1;
        let hint = hint.as_ref();
        if hint.is_empty() {
            println!("  \u{2717} {}", msg.as_ref());
        } else {
            println!("  \u{2717} {}\n      {hint}", msg.as_ref());
        }
    }
}

pub fn run(home: &Home, global: &GlobalArgs) -> Result<()> {
    let mut r = Report::default();
    println!("Tool");
    r.ok(format!(
        "idf-next {} (supports ESP-IDF {})",
        env!("CARGO_PKG_VERSION"),
        idf_support::SUPPORTED_IDF_RANGE
    ));
    match platform::host() {
        Ok(p) => r.ok(format!("host platform {p}")),
        Err(e) => r.fail(e.to_string(), ""),
    }
    match Uv::locate(home).and_then(|uv| Ok((uv.version()?, uv.exe().to_path_buf()))) {
        Ok((v, path)) => r.ok(format!("{v} ({})", path.display())),
        Err(e) => r.fail(e.to_string(), ""),
    }
    r.ok(format!("SDK cache {}", home.root().display()));

    match Project::discover(global.project.as_deref()) {
        Ok(Some(project)) => check_project(&mut r, home, &project),
        Ok(None) => check_installed(&mut r, home),
        Err(e) => {
            println!("Project");
            r.fail(format!("{e:?}"), "");
        }
    }

    println!();
    match (r.failures, r.warnings) {
        (0, 0) => {
            println!("No problems found.");
            Ok(())
        }
        (0, w) => {
            println!("{w} warning(s).");
            Ok(())
        }
        (f, _) => bail!("{f} problem(s) found"),
    }
}

fn check_installed(r: &mut Report, home: &Home) {
    println!("Installed SDKs");
    let versions = sdk::installed_versions(home);
    if versions.is_empty() {
        r.ok("none installed (run `idf sdk install <version>` or use a project)");
    }
    for (v, complete) in versions {
        if !complete {
            r.warn(
                format!("ESP-IDF {v} is incomplete"),
                format!("run `idf sdk install {v}` or `idf sdk remove {v}`"),
            );
        } else if !idf_support::supports(&v) {
            r.warn(
                format!("ESP-IDF {v} is outside the supported range"),
                format!("run `idf sdk remove {v}`"),
            );
        } else {
            r.ok(format!("ESP-IDF {v}"));
        }
    }
}

fn check_project(r: &mut Report, home: &Home, project: &Project) {
    println!("Project ({})", project.root.display());
    let m = &project.manifest;
    r.ok(format!(
        "idf.toml: {} for {}, ESP-IDF {}",
        m.project.name.get_ref(),
        m.project.target,
        m.project.idf.get_ref()
    ));
    let Some(lock) = &project.lock else {
        r.fail(
            "idf.lock is missing",
            "run `idf sdk install` to pin and install the SDK",
        );
        return;
    };
    let Some(lock) = project.current_lock() else {
        r.fail(
            format!(
                "idf.lock pins ESP-IDF {}, but idf.toml asks for {}",
                lock.idf.version,
                project.idf_spec()
            ),
            format!("run `idf sdk use {}` to re-pin", project.idf_spec()),
        );
        return;
    };
    r.ok(format!(
        "idf.lock: ESP-IDF {} ({}), Python {}",
        lock.idf.version,
        &lock.idf.commit.as_str()[..10],
        lock.python.version
    ));

    println!("SDK");
    let sdk = match Sdk::load(home, &lock.idf.version) {
        Ok(Some(sdk)) => sdk,
        Ok(None) => {
            r.fail(
                format!("ESP-IDF {} is not installed", lock.idf.version),
                "run `idf sdk install`",
            );
            return;
        }
        Err(e) => {
            r.fail(format!("cannot read the installed SDK: {e}"), "");
            return;
        }
    };
    if sdk.record.idf != lock.idf {
        r.fail(
            format!(
                "installed ESP-IDF {} differs from idf.lock",
                lock.idf.version
            ),
            format!(
                "run `idf sdk remove {0}` then `idf sdk install`",
                lock.idf.version
            ),
        );
        return;
    }
    r.ok(format!(
        "ESP-IDF {} at {}",
        lock.idf.version,
        sdk.idf_path().display()
    ));

    let Some(python) = python::existing(&sdk.dir, &lock.python.requirements) else {
        r.fail(
            "the Python environment from idf.lock is not installed",
            "run `idf sdk install`",
        );
        return;
    };
    let installed = Installed { sdk, python };
    let env = installed.environment();
    for tool in &installed.sdk.tools {
        check_tool(r, &installed, tool, &env);
    }

    println!("Python");
    let py = installed.python.python();
    match Command::new(&py)
        .args(["-c", "import sys; print(sys.version.split()[0])"])
        .output()
    {
        Ok(o) if o.status.success() => r.ok(format!(
            "Python {} ({})",
            String::from_utf8_lossy(&o.stdout).trim(),
            installed.python.venv().display()
        )),
        _ => {
            r.fail(
                format!("{} does not run", py.display()),
                "run `idf sdk remove` and `idf sdk install`",
            );
            return;
        }
    }
    // IDF's own check, the one CMake runs before every build.
    let mut cmd = Command::new(&py);
    cmd.arg(installed.sdk.idf_path().join("tools/idf_tools.py"))
        .arg("check-python-dependencies");
    env.apply(&mut cmd);
    match cmd.output() {
        Ok(o) if o.status.success() => r.ok("ESP-IDF's check-python-dependencies passes"),
        Ok(o) => r.fail(
            "ESP-IDF's check-python-dependencies fails",
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout).trim(),
                String::from_utf8_lossy(&o.stderr).trim()
            ),
        ),
        Err(e) => r.fail(format!("cannot run idf_tools.py: {e}"), ""),
    }
}

fn check_tool(r: &mut Report, installed: &Installed, tool: &Selected, env: &sdk::Environment) {
    let dir = tool.install_dir(&installed.sdk.dir);
    if !dir.is_dir() {
        r.fail(
            format!("{} {} is missing", tool.name, tool.version),
            "run `idf sdk install`",
        );
        return;
    }
    let Some((exe, args)) = tool
        .version_cmd
        .split_first()
        .filter(|(e, _)| !e.is_empty())
    else {
        r.ok(format!("{} {}", tool.name, tool.version));
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(args);
    env.apply(&mut cmd);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            r.fail(
                format!("{} {}: cannot run `{exe}`: {e}", tool.name, tool.version),
                "",
            );
            return;
        }
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let found = regex::Regex::new(&tool.version_regex).ok().and_then(|re| {
        re.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_owned())
    });
    match found {
        Some(v) if v == tool.version => r.ok(format!("{} {}", tool.name, tool.version)),
        Some(v) => r.fail(
            format!(
                "{}: `{exe}` reports {v}, expected {}",
                tool.name, tool.version
            ),
            "run `idf sdk install`",
        ),
        None => r.warn(
            format!(
                "{} {}: could not read the version of `{exe}`",
                tool.name, tool.version
            ),
            text.trim(),
        ),
    }
}
