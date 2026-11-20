//! `idf add`, `remove`, `update` and `tree`.

use crate::build;
use crate::cli::GlobalArgs;
use crate::commands::build::{Prepared, prepare, record_dependencies};
use crate::deps::{self, Solved};
use crate::manifest::{Dependency, DependencySource};
use crate::sdk::Ctx;
use crate::ui;
use miette::{Result, bail, miette};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Write the generated project for the current `idf.toml` and run the
/// component manager on it.
fn resolve(p: &Prepared) -> Result<Solved> {
    build::generate(&p.project, &p.layout)?;
    deps::solve(&p.build())?
        .ok_or_else(|| miette!("the component manager wrote no dependencies.lock"))
}

fn forbid_locked(global: &GlobalArgs, what: &str) -> Result<()> {
    if global.locked {
        bail!("`idf {what}` would change idf.toml and idf.lock, but --locked was given");
    }
    Ok(())
}

pub fn add(
    ctx: &Ctx,
    global: &GlobalArgs,
    specs: &[String],
    git: Option<&str>,
    rev: Option<&str>,
    path: Option<&Path>,
) -> Result<()> {
    forbid_locked(global, "add")?;
    let mut p = prepare(ctx, global, "add")?;
    let sourced = git.is_some() || path.is_some();
    if sourced && specs.len() != 1 {
        bail!("--git and --path take exactly one dependency name");
    }
    // (name, dependency, whether the requirement is still open)
    let mut wanted: Vec<(String, Dependency, bool)> = Vec::new();
    for spec in specs {
        let (name, req) = match spec.split_once('@') {
            Some((n, r)) if !sourced => (n, Some(r)),
            _ => (spec.as_str(), None),
        };
        if name.is_empty() || req.is_some_and(str::is_empty) {
            bail!(
                help = "Write namespace/name or namespace/name@requirement.",
                "{spec:?} is not a dependency"
            );
        }
        let (source, open) = match (git, path) {
            (Some(url), _) => (
                DependencySource::Git {
                    url: url.to_owned(),
                    rev: rev.map(str::to_owned),
                    subdir: None,
                },
                false,
            ),
            (None, Some(path)) => {
                let dir = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
                if dir.file_name().is_some_and(|d| d != name) {
                    bail!(
                        help = format!(
                            "Name the dependency after its directory: `idf add {} --path {}`.",
                            dir.file_name().unwrap_or_default().to_string_lossy(),
                            path.display()
                        ),
                        "a path dependency is named after the component's directory, but {name} is not {}",
                        dir.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
                (
                    DependencySource::Path {
                        path: relative_to(&p.project.root, path),
                    },
                    false,
                )
            }
            (None, None) => (
                DependencySource::Registry {
                    version: req.unwrap_or("*").to_owned(),
                },
                req.is_none(),
            ),
        };
        wanted.push((
            name.to_owned(),
            Dependency {
                source,
                targets: None,
                idf: None,
            },
            open,
        ));
    }

    // Resolve with the new entries in place, to learn the versions that
    // open requirements settle on.
    ui::status("Resolving", "dependencies");
    let mut trial = p.project.manifest.clone();
    for (name, dep, _) in &wanted {
        trial.dependencies.insert(name.clone(), dep.clone());
    }
    build::generate(&p.project, &p.layout)?;
    deps::sync_files(&trial, p.project.lock.as_ref(), &p.layout)?;
    let solved = deps::solve(&p.build())?
        .ok_or_else(|| miette!("the component manager wrote no dependencies.lock"))?;

    let mut edits = Vec::new();
    for (name, mut dep, open) in wanted {
        let Some(found) = solved.components.iter().find(|c| c.locked.name == name) else {
            bail!("{name} was not resolved; is it spelled namespace/name?");
        };
        if open && let DependencySource::Registry { version } = &mut dep.source {
            *version = format!("^{}", found.locked.version);
        }
        ui::status("Adding", format!("{name} {}", describe(&dep)));
        edits.push((name, Some(dep)));
    }
    p.project.edit_dependencies(&edits)?;
    finish(&mut p)
}

pub fn remove(ctx: &Ctx, global: &GlobalArgs, names: &[String]) -> Result<()> {
    forbid_locked(global, "remove")?;
    let mut p = prepare(ctx, global, "remove")?;
    for n in names {
        if !p.project.manifest.dependencies.contains_key(n) {
            let have: Vec<&str> = p
                .project
                .manifest
                .dependencies
                .keys()
                .map(String::as_str)
                .collect();
            bail!(
                help = if have.is_empty() {
                    "idf.toml has no [dependencies].".to_owned()
                } else {
                    format!("Dependencies: {}.", have.join(", "))
                },
                "{n} is not a dependency of this project"
            );
        }
        ui::status("Removing", n.as_str());
    }
    let edits: Vec<_> = names.iter().map(|n| (n.clone(), None)).collect();
    p.project.edit_dependencies(&edits)?;
    finish(&mut p)
}

pub fn update(ctx: &Ctx, global: &GlobalArgs, names: &[String]) -> Result<()> {
    forbid_locked(global, "update")?;
    let mut p = prepare(ctx, global, "update")?;
    for n in names {
        if !p.project.manifest.dependencies.contains_key(n) {
            bail!("{n} is not a dependency of this project");
        }
    }
    let before: BTreeMap<String, String> = p
        .project
        .lock
        .iter()
        .flat_map(|l| &l.components)
        .map(|c| (c.name.clone(), c.version.clone()))
        .collect();
    // Drop the pins to be refreshed: everything, or the named dependencies
    // together with what only they pulled in.
    if let Some(mut lock) = p.project.lock.clone() {
        build::generate(&p.project, &p.layout)?;
        let drop = if names.is_empty() {
            None
        } else {
            Some(deps::read_solved(&p.build())?.map(|s| closure(&s, names)))
        };
        lock.components = match drop {
            None => Vec::new(),
            Some(drop) => {
                let drop = drop.unwrap_or_else(|| names.to_vec());
                lock.components
                    .iter()
                    .filter(|c| !drop.contains(&c.name))
                    .cloned()
                    .collect()
            }
        };
        p.project.save_lock(lock)?;
        // The build directory's lock must not win over the new pins.
        let _ = std::fs::remove_file(p.layout.project_dir().join("dependencies.lock"));
    }
    ui::status("Updating", "dependencies");
    finish(&mut p)?;
    let mut changed = false;
    for c in p.project.lock.iter().flat_map(|l| &l.components) {
        match before.get(&c.name) {
            Some(old) if *old == c.version => {}
            Some(old) => {
                ui::status("Updated", format!("{} {old} -> {}", c.name, c.version));
                changed = true;
            }
            None => {
                ui::status("Added", format!("{} {}", c.name, c.version));
                changed = true;
            }
        }
    }
    if !changed {
        ui::status("Unchanged", "everything is up to date");
    }
    Ok(())
}

/// Solve for the current `idf.toml` and record the result in `idf.lock`.
fn finish(p: &mut Prepared) -> Result<()> {
    if p.project.manifest.dependencies.is_empty() {
        build::generate(&p.project, &p.layout)?;
    } else {
        resolve(p)?;
    }
    record_dependencies(p, false)
}

pub fn tree(ctx: &Ctx, global: &GlobalArgs) -> Result<()> {
    let p = prepare(ctx, global, "tree")?;
    if p.project.manifest.dependencies.is_empty() {
        ui::status("Tree", "no [dependencies] in idf.toml");
        return Ok(());
    }
    // Prefer what the last configure solved; solve when there is nothing yet.
    build::generate(&p.project, &p.layout)?;
    let solved = match deps::read_solved(&p.build())? {
        Some(s) => s,
        None => {
            if global.offline {
                bail!("there is no resolved tree yet and --offline forbids resolving");
            }
            resolve(&p)?
        }
    };
    let by_name: BTreeMap<&str, &deps::SolvedComponent> = solved
        .components
        .iter()
        .map(|c| (c.locked.name.as_str(), c))
        .collect();
    println!(
        "{} (ESP-IDF {})",
        p.project.manifest.project.name.get_ref(),
        p.installed.sdk.record.idf.version
    );
    let top: Vec<&str> = solved.direct.iter().map(String::as_str).collect();
    print_level(&by_name, &top, "", &mut Vec::new());
    Ok(())
}

fn print_level<'a>(
    by_name: &BTreeMap<&'a str, &'a deps::SolvedComponent>,
    names: &[&'a str],
    prefix: &str,
    seen: &mut Vec<&'a str>,
) {
    for (i, name) in names.iter().enumerate() {
        let last = i + 1 == names.len();
        let branch = if last { "└── " } else { "├── " };
        let Some(c) = by_name.get(name) else {
            println!("{prefix}{branch}{name} (not resolved)");
            continue;
        };
        let repeated = seen.contains(name);
        let star = if repeated && !c.requires.is_empty() {
            " (*)"
        } else {
            ""
        };
        println!(
            "{prefix}{branch}{} {}{star}",
            c.locked.name, c.locked.version
        );
        if !repeated {
            seen.push(name);
            let kids: Vec<&str> = c.requires.iter().map(|(n, _)| n.as_str()).collect();
            let next = format!("{prefix}{}", if last { "    " } else { "│   " });
            print_level(by_name, &kids, &next, seen);
        }
    }
}

/// The named components and what only they pull in.
fn closure(s: &Solved, names: &[String]) -> Vec<String> {
    let by_name: BTreeMap<&str, &deps::SolvedComponent> = s
        .components
        .iter()
        .map(|c| (c.locked.name.as_str(), c))
        .collect();
    let reach = |roots: Vec<&str>| -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut stack = roots;
        while let Some(n) = stack.pop() {
            if seen.iter().any(|o| o == n) {
                continue;
            }
            seen.push(n.to_owned());
            if let Some(c) = by_name.get(n) {
                stack.extend(c.requires.iter().map(|(r, _)| r.as_str()));
            }
        }
        seen
    };
    let drop = reach(names.iter().map(String::as_str).collect());
    let others: Vec<&str> = s
        .direct
        .iter()
        .map(String::as_str)
        .filter(|d| !names.iter().any(|n| n == d))
        .collect();
    let needed = reach(others);
    drop.into_iter().filter(|d| !needed.contains(d)).collect()
}

fn describe(d: &Dependency) -> String {
    match &d.source {
        DependencySource::Registry { version } => version.clone(),
        DependencySource::Git { url, rev, .. } => format!(
            "{url}{}",
            rev.as_deref().map(|r| format!("#{r}")).unwrap_or_default()
        ),
        DependencySource::Path { path } => format!("path {path}"),
    }
}

/// `path` as written in idf.toml: relative to the project root when it is
/// inside it.
fn relative_to(root: &Path, path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
    match abs.strip_prefix(root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => abs.display().to_string(),
    }
}
