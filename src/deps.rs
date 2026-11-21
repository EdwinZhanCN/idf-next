//! `[dependencies]` and the IDF component manager.
//!
//! `idf.toml` is translated into the `idf_component.yml` of the generated
//! `main` component, and the manager (from the SDK's venv) runs inside
//! `build/<target>/<profile>/project/`, so `managed_components/` and
//! `dependencies.lock` stay in the build directory. What the manager solved is
//! merged into `idf.lock` (`[[component]]`) and seeded back into a fresh build
//! directory, so a checkout builds the same versions.

use crate::build::{Build, Layout};
use crate::chip::Chip;
use crate::fsx;
use crate::lockfile::{ComponentSource, Digest, LockedComponent, Lockfile};
use crate::manifest::{Dependency, DependencySource, Manifest};
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Registry the manager uses by default.
pub const REGISTRY: &str = "https://components.espressif.com/";

const MANIFEST_NAME: &str = "idf_component.yml";
const LOCK_NAME: &str = "dependencies.lock";

// -------------------------------------------------------- idf.toml -> yml ---

/// The `idf_component.yml` for `main`, or `None` without dependencies. JSON is
/// valid YAML, so no YAML writer is needed.
pub fn component_manifest(m: &Manifest, root: &Path, pins: &[LockedComponent]) -> Option<String> {
    if m.dependencies.is_empty() {
        return None;
    }
    let deps: serde_json::Map<String, Value> = m
        .dependencies
        .iter()
        .map(|(name, d)| {
            let pin = pins.iter().find(|c| c.name == *name);
            (name.clone(), dependency_json(d, root, pin))
        })
        .collect();
    let doc = json!({ "dependencies": deps });
    Some(serde_json::to_string_pretty(&doc).expect("json") + "\n")
}

fn dependency_json(d: &Dependency, root: &Path, pin: Option<&LockedComponent>) -> Value {
    let mut rules = Vec::new();
    if let Some(targets) = &d.targets {
        let list: Vec<&str> = targets.iter().map(Chip::as_str).collect();
        rules.push(json!({ "if": format!("target in [{}]", list.join(", ")) }));
    }
    if let Some(idf) = &d.idf {
        let cond = match idf.patch {
            Some(p) => format!("idf_version =={}.{}.{p}", idf.major, idf.minor),
            None => format!(
                "idf_version >={}.{}.0,<{}.{}.0",
                idf.major,
                idf.minor,
                idf.major,
                idf.minor + 1
            ),
        };
        rules.push(json!({ "if": cond }));
    }
    let mut table = serde_json::Map::new();
    match &d.source {
        DependencySource::Registry { version } => {
            if rules.is_empty() {
                return json!(version);
            }
            table.insert("version".into(), json!(version));
        }
        DependencySource::Git { url, rev, subdir } => {
            table.insert("git".into(), json!(url));
            // A locked commit beats the branch or tag in idf.toml.
            let pinned = pin.and_then(|c| match &c.source {
                ComponentSource::Git { url: u, commit } if u == url => Some(commit.as_str()),
                _ => None,
            });
            if let Some(rev) = pinned.or(rev.as_deref()) {
                table.insert("version".into(), json!(rev));
            }
            if let Some(subdir) = subdir {
                table.insert("path".into(), json!(subdir));
            }
        }
        DependencySource::Path { path } => {
            let abs = std::path::absolute(root.join(path)).unwrap_or_else(|_| root.join(path));
            table.insert("path".into(), json!(abs));
        }
    }
    if !rules.is_empty() {
        table.insert("rules".into(), json!(rules));
    }
    Value::Object(table)
}

// ------------------------------------------------------------------ solved ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Solved {
    /// Everything the manager resolved except the SDK itself.
    pub components: Vec<SolvedComponent>,
    /// Names of the project's own (direct) dependencies.
    pub direct: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolvedComponent {
    pub locked: LockedComponent,
    /// `(name, requirement)` of what it depends on, without the SDK.
    pub requires: Vec<(String, String)>,
}

impl Solved {
    /// Parse the JSON form of the manager's `dependencies.lock`.
    pub fn from_json(v: &Value, layout: &Layout) -> Result<Self> {
        let mut components = Vec::new();
        let deps = v["dependencies"].as_object().cloned().unwrap_or_default();
        for (name, c) in deps {
            let source = &c["source"];
            let version = c["version"].as_str().unwrap_or_default().to_owned();
            let ty = source["type"].as_str().unwrap_or_default();
            let (src, hash) = match ty {
                "idf" => continue,
                "service" => (
                    ComponentSource::Registry {
                        url: source["registry_url"]
                            .as_str()
                            .unwrap_or(REGISTRY)
                            .to_owned(),
                    },
                    c["component_hash"]
                        .as_str()
                        .map(|h| format!("sha256:{h}").parse::<Digest>())
                        .transpose()
                        .map_err(|e| miette::miette!("{name}: {e}"))?,
                ),
                "git" => {
                    let url = source["git"].as_str().unwrap_or_default().to_owned();
                    let commit = c["source"]["git_commit"]
                        .as_str()
                        .or(Some(version.as_str()))
                        .unwrap_or_default();
                    let commit = commit
                        .parse()
                        .map_err(|e| miette::miette!("{name}: git commit: {e}"))?;
                    (ComponentSource::Git { url, commit }, None)
                }
                "local" => (
                    ComponentSource::Path {
                        path: project_relative(layout, source["path"].as_str().unwrap_or_default()),
                    },
                    None,
                ),
                other => {
                    return Err(miette::miette!(
                        "{name}: the component manager reported source type {other:?}"
                    ));
                }
            };
            let requires = c["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| {
                    let n = d["name"].as_str()?;
                    (n != "idf").then(|| {
                        (
                            n.to_owned(),
                            d["version"].as_str().unwrap_or("*").to_owned(),
                        )
                    })
                })
                .collect();
            components.push(SolvedComponent {
                locked: LockedComponent {
                    name,
                    version,
                    source: src,
                    hash,
                },
                requires,
            });
        }
        components.sort_by(|a, b| a.locked.name.cmp(&b.locked.name));
        let direct = v["direct_dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().map(str::to_owned))
            .collect();
        Ok(Solved { components, direct })
    }

    pub fn locked(&self) -> Vec<LockedComponent> {
        self.components.iter().map(|c| c.locked.clone()).collect()
    }
}

/// The manager records local paths relative to the generated project; the
/// lock file wants them as written in `idf.toml`, relative to the project.
fn project_relative(layout: &Layout, path: &str) -> String {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        layout.project_dir().join(p)
    };
    let mut norm = std::path::PathBuf::new();
    for c in joined.components() {
        match c {
            std::path::Component::ParentDir => {
                norm.pop();
            }
            std::path::Component::CurDir => {}
            other => norm.push(other),
        }
    }
    match norm.strip_prefix(&layout.root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => norm.display().to_string(),
    }
}

/// Read `dependencies.lock` through the SDK's Python (it has the YAML library).
pub fn read_solved(b: &Build) -> Result<Option<Solved>> {
    let path = b.layout.project_dir().join(LOCK_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    let mut cmd = std::process::Command::new(b.python());
    b.env().apply(&mut cmd);
    cmd.args([
        "-I",
        "-c",
        "import json,sys,ruamel.yaml; \
         print(json.dumps(ruamel.yaml.YAML(typ='safe').load(open(sys.argv[1]))))",
    ])
    .arg(&path);
    let out = cmd
        .output()
        .into_diagnostic()
        .wrap_err("reading dependencies.lock")?;
    if !out.status.success() {
        bail!(
            "reading {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: Value = serde_json::from_slice(&out.stdout).into_diagnostic()?;
    Solved::from_json(&v, &b.layout).map(Some)
}

// --------------------------------------------------------- idf.lock <-> dir ---

fn digest_of(components: &[LockedComponent]) -> String {
    let mut text = String::new();
    for c in components {
        text += &format!("{} {} {} {:?}\n", c.name, c.version, c.source, c.hash);
    }
    Digest::of(text.as_bytes()).to_string()
}

fn stamp_path(layout: &Layout) -> std::path::PathBuf {
    layout.dir().join("deps.stamp")
}

/// Write the generated project's component files: `main/idf_component.yml`,
/// and a `dependencies.lock` carrying what `idf.lock` pins when the build
/// directory has not seen those pins yet. Returns whether anything changed.
pub fn sync_files(m: &Manifest, lock: Option<&Lockfile>, layout: &Layout) -> Result<bool> {
    let pdir = layout.project_dir();
    let manifest_path = pdir.join("main").join(MANIFEST_NAME);
    let mut changed = false;
    match component_manifest(m, &layout.root, lock.map_or(&[], |l| &l.components)) {
        Some(text) => {
            if std::fs::read_to_string(&manifest_path).ok().as_deref() != Some(&text) {
                fsx::create_dir_all(pdir.join("main"))?;
                fsx::write(&manifest_path, text)?;
                changed = true;
            }
        }
        None => {
            if manifest_path.exists() {
                std::fs::remove_file(&manifest_path).into_diagnostic()?;
                for stale in [LOCK_NAME, "managed_components"] {
                    let p = pdir.join(stale);
                    if p.is_dir() {
                        fsx::remove_dir_all(&p)?;
                    } else if p.exists() {
                        std::fs::remove_file(&p).into_diagnostic()?;
                    }
                }
                let _ = std::fs::remove_file(stamp_path(layout));
                changed = true;
            }
            return Ok(changed);
        }
    }
    let pinned: Vec<LockedComponent> = lock.map(|l| l.components.clone()).unwrap_or_default();
    if pinned.is_empty() {
        return Ok(changed);
    }
    let want = digest_of(&pinned);
    let have = std::fs::read_to_string(stamp_path(layout)).ok();
    if have.as_deref() == Some(&want) && pdir.join(LOCK_NAME).is_file() {
        return Ok(changed);
    }
    let direct: Vec<&str> = m.dependencies.keys().map(String::as_str).collect();
    let mut deps = serde_json::Map::new();
    for c in &pinned {
        let entry = match &c.source {
            ComponentSource::Registry { url } => {
                let mut e = json!({
                    "source": { "registry_url": url, "type": "service" },
                    "version": c.version,
                });
                if let Some(h) = &c.hash {
                    e["component_hash"] = json!(h.as_str().trim_start_matches("sha256:"));
                }
                e
            }
            // Git pins go into the generated manifest instead (see
            // `component_manifest`): the manager re-resolves branches whenever
            // it solves, but follows an explicit commit.
            ComponentSource::Git { .. } => continue,
            // Local components are read from disk; there is nothing to pin.
            ComponentSource::Path { .. } => continue,
        };
        deps.insert(c.name.clone(), entry);
    }
    let doc = json!({
        "dependencies": deps,
        "direct_dependencies": direct,
        "target": layout.target.as_str(),
        "version": "3.0.0",
    });
    // No manifest_hash: the manager solves again, but starts from these pins.
    fsx::write(
        pdir.join(LOCK_NAME),
        serde_json::to_string_pretty(&doc).expect("json") + "\n",
    )?;
    Ok(true)
}

/// Whether `idf.lock` has already been brought up to date with the build
/// directory's `dependencies.lock` (the stamp is newer than it).
pub fn recorded(layout: &Layout) -> bool {
    let mtime = |p: std::path::PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (
        mtime(stamp_path(layout)),
        mtime(layout.project_dir().join(LOCK_NAME)),
    ) {
        (Some(stamp), Some(lock)) => stamp >= lock,
        _ => false,
    }
}

/// Record what was solved, so the next checkout starts from it.
pub fn remember(layout: &Layout, components: &[LockedComponent]) -> Result<()> {
    fsx::create_dir_all(layout.dir())?;
    fsx::write(stamp_path(layout), digest_of(components))
}

/// Replace the components in `lock` by `solved`. Entries that do not apply to
/// this target are kept when some dependency is restricted to chips (a lock
/// is shared by all targets).
pub fn merge(lock: &mut Lockfile, m: &Manifest, solved: &[LockedComponent]) -> bool {
    let restricted = m.dependencies.values().any(|d| d.targets.is_some());
    let mut map: BTreeMap<String, LockedComponent> = if restricted {
        lock.components
            .iter()
            .map(|c| (c.name.clone(), c.clone()))
            .collect()
    } else {
        BTreeMap::new()
    };
    for c in solved {
        map.insert(c.name.clone(), c.clone());
    }
    let mut new: Vec<LockedComponent> = map.into_values().collect();
    new.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.source.cmp(&b.source)));
    let mut old = lock.components.clone();
    old.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.source.cmp(&b.source)));
    let changed = old != new;
    lock.components = new;
    changed
}

// ------------------------------------------------------------- the manager ---

/// Run the manager's dependency step on its own (what CMake does at
/// configure time): solve, download, write `dependencies.lock`.
pub fn solve(b: &Build) -> Result<Option<Solved>> {
    let pdir = b.layout.project_dir();
    let tmp = tempfile::tempdir().into_diagnostic()?;
    let local = tmp.path().join("local_components.yml");
    let managed = tmp.path().join("managed_components_list.cmake");
    let mut text = format!(
        "components:\n  - name: \"main\"\n    path: \"{}\"\n",
        pdir.join("main").display()
    );
    let user = b.layout.root.join("components");
    if let Ok(rd) = std::fs::read_dir(&user) {
        let mut dirs: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for d in dirs {
            let name = d
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            text += &format!("  - name: \"{name}\"\n    path: \"{}\"\n", d.display());
        }
    }
    fsx::write(&local, text)?;
    let mut cmd = std::process::Command::new(b.python());
    b.env().apply(&mut cmd);
    cmd.args(["-m", "idf_component_manager.prepare_components"])
        .arg(format!("--project_dir={}", pdir.display()))
        .arg(format!("--lock_path={}", pdir.join(LOCK_NAME).display()))
        .arg("--interface_version=4");
    // The values of the last configure, for `matches:` rules on Kconfig.
    let sdkconfig = b.layout.dir().join("config/sdkconfig.json");
    if sdkconfig.is_file() {
        cmd.arg(format!("--sdkconfig_json_file={}", sdkconfig.display()));
    }
    cmd.arg("prepare_dependencies")
        .arg(format!("--local_components_list_file={}", local.display()))
        .arg(format!(
            "--managed_components_list_file={}",
            managed.display()
        ));
    run_manager(cmd)?;
    read_solved(b)
}

/// Run the manager; on failure show what it complained about, without its
/// progress notes (`-v` shows everything).
fn run_manager(mut cmd: std::process::Command) -> Result<()> {
    if crate::ui::verbosity() >= crate::ui::Verbosity::Verbose {
        return crate::build::run_captured(cmd, "Resolving dependencies");
    }
    let out = cmd
        .output()
        .into_diagnostic()
        .wrap_err("running the component manager")?;
    if out.status.success() {
        return Ok(());
    }
    let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    bail!("Resolving dependencies failed:\n{}", manager_errors(&log));
}

/// The manager's output without `NOTE:` lines, progress dots, and the
/// complaint about the SDK having no `.git`.
fn manager_errors(log: &str) -> String {
    let lines: Vec<&str> = log
        .lines()
        .map(|l| l.trim_start_matches('.'))
        .filter(|l| {
            let l = l.trim();
            !l.is_empty()
                && !l.starts_with("NOTE:")
                && !l.contains("esp-idf/.git")
                && !l.contains("Git version unavailable")
        })
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(deps: &str) -> Manifest {
        let text = format!(
            "[project]\nname = \"p\"\ntarget = \"esp32p4\"\nidf = \"6.0\"\n\n[dependencies]\n{deps}"
        );
        Manifest::parse(&text, "idf.toml").unwrap()
    }

    fn yaml(m: &Manifest, pins: &[LockedComponent]) -> Value {
        serde_json::from_str(&component_manifest(m, Path::new("/proj"), pins).unwrap()).unwrap()
    }

    #[test]
    fn errors_drop_progress_notes() {
        let log = "NOTE: Dependencies lock doesn't exist, solving dependencies.\n\
                   .......fatal: not a git repository: '/sdk/esp-idf/.git'\n\
                   WARNING: Git version unavailable, reading from source\n\
                   ERROR: Version solving failed:\n    - no versions of x/y match *\n";
        assert_eq!(
            manager_errors(log),
            "ERROR: Version solving failed:\n    - no versions of x/y match *"
        );
    }

    #[test]
    fn no_dependencies_no_manifest() {
        let m = Manifest::parse(
            "[project]\nname = \"p\"\ntarget = \"esp32\"\nidf = \"6.0\"\n",
            "idf.toml",
        )
        .unwrap();
        assert!(component_manifest(&m, Path::new("/proj"), &[]).is_none());
    }

    #[test]
    fn translates_each_source() {
        let m = manifest(
            "a = \"^1.0\"\n\
             b = { version = \"2\", targets = [\"esp32s3\", \"esp32p4\"], idf = \"6.0\" }\n\
             c = { git = \"https://x/y.git\", rev = \"main\", subdir = \"c\" }\n\
             d = { path = \"../d\" }\n",
        );
        let y = yaml(&m, &[]);
        let d = &y["dependencies"];
        assert_eq!(d["a"], json!("^1.0"));
        assert_eq!(d["b"]["version"], json!("2"));
        assert_eq!(
            d["b"]["rules"][0]["if"],
            json!("target in [esp32s3, esp32p4]")
        );
        assert_eq!(
            d["b"]["rules"][1]["if"],
            json!("idf_version >=6.0.0,<6.1.0")
        );
        assert_eq!(d["c"]["git"], json!("https://x/y.git"));
        assert_eq!(d["c"]["version"], json!("main"));
        assert_eq!(d["c"]["path"], json!("c"));
        assert_eq!(d["d"]["path"], json!("/proj/../d"));
    }

    #[test]
    fn a_locked_commit_replaces_the_branch() {
        let m = manifest("c = { git = \"https://x/y.git\", rev = \"main\" }\n");
        let commit = "ab".repeat(20);
        let pin = LockedComponent {
            name: "c".into(),
            version: commit.clone(),
            source: format!("git+https://x/y.git#{commit}").parse().unwrap(),
            hash: None,
        };
        assert_eq!(
            yaml(&m, std::slice::from_ref(&pin))["dependencies"]["c"]["version"],
            json!(commit)
        );
        // A pin for another URL is ignored.
        let other = LockedComponent {
            source: format!("git+https://z/y.git#{commit}").parse().unwrap(),
            ..pin
        };
        assert_eq!(
            yaml(&m, &[other])["dependencies"]["c"]["version"],
            json!("main")
        );
    }

    #[test]
    fn reads_the_manager_lock() {
        let hash = "cd".repeat(32);
        let v = json!({
            "dependencies": {
                "espressif/button": {
                    "component_hash": hash,
                    "dependencies": [
                        {"name": "espressif/cmake_utilities", "version": "*"},
                        {"name": "idf", "version": ">=4.0"},
                    ],
                    "source": {"registry_url": "https://components.espressif.com/", "type": "service"},
                    "version": "4.2.3",
                },
                "idf": {"source": {"type": "idf"}, "version": "6.0.3"},
                "local": {
                    "dependencies": [],
                    "source": {"path": "../../../../comp", "type": "local"},
                    "version": "0.1.0",
                },
            },
            "direct_dependencies": ["espressif/button", "local"],
        });
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path(), &"esp32p4".parse().unwrap(), "dev");
        let s = Solved::from_json(&v, &layout).unwrap();
        assert_eq!(s.direct, ["espressif/button", "local"]);
        assert_eq!(s.components.len(), 2, "the SDK itself is not a component");
        let button = &s.components[0];
        assert_eq!(button.locked.version, "4.2.3");
        assert_eq!(
            button.locked.hash.as_ref().unwrap().as_str(),
            format!("sha256:{hash}")
        );
        assert_eq!(
            button.requires,
            [("espressif/cmake_utilities".to_owned(), "*".to_owned())]
        );
    }

    #[test]
    fn merge_replaces_unless_targets_restrict() {
        let hash: Digest = format!("sha256:{}", "ab".repeat(32)).parse().unwrap();
        let comp = |name: &str, version: &str| LockedComponent {
            name: name.into(),
            version: version.into(),
            source: "registry+https://components.espressif.com/"
                .parse()
                .unwrap(),
            hash: Some(hash.clone()),
        };
        let plain = manifest("a = \"1\"\n");
        let restricted = manifest("a = \"1\"\nb = { version = \"1\", targets = [\"esp32s3\"] }\n");
        let lock_text = |c: &[LockedComponent]| Lockfile {
            version: crate::lockfile::FORMAT_VERSION,
            idf: crate::lockfile::LockedIdf {
                version: semver::Version::new(6, 0, 3),
                commit: "0123456789abcdef0123456789abcdef01234567".parse().unwrap(),
                source: Digest::of(b"source"),
            },
            python: crate::lockfile::LockedPython {
                version: semver::Version::new(3, 12, 11),
                exclude_newer: "2026-11-13T09:00:00Z".into(),
                constraints: Digest::of(b"c"),
                requirements: Digest::of(b"r"),
            },
            tools: Vec::new(),
            components: c.to_vec(),
        };

        let mut lock = lock_text(&[comp("a", "1.0.0"), comp("gone", "1.0.0")]);
        assert!(merge(&mut lock, &plain, &[comp("a", "1.1.0")]));
        assert_eq!(lock.components, [comp("a", "1.1.0")]);
        assert!(
            !merge(&mut lock, &plain, &[comp("a", "1.1.0")]),
            "no change"
        );

        let mut lock = lock_text(&[comp("a", "1.0.0"), comp("b", "1.0.0")]);
        merge(&mut lock, &restricted, &[comp("a", "1.1.0")]);
        assert_eq!(lock.components, [comp("a", "1.1.0"), comp("b", "1.0.0")]);
    }
}
