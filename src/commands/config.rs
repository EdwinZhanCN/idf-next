//! `idf config`: read Kconfig through the build, write it to `idf.toml`.
//!
//! `idf.toml` is the only place configuration lives. `set` and `menu` write
//! to its `[config]` table (or `[profile.<name>.config]` with `-r` or
//! `--profile`); the next build starts `sdkconfig` over from it.

use crate::build::{self, Layout};
use crate::cli::{ConfigAction, GlobalArgs};
use crate::commands::build::{Prepared, prepare};
use crate::kconfig::{self, Evaluator, Kind, Menus, Symbol, Values};
use crate::manifest::{ConfigKey, ConfigValue};
use crate::sdk::Ctx;
use crate::{fsx, ui};
use miette::Result;
use std::collections::BTreeMap;

pub fn run(ctx: &Ctx, global: &GlobalArgs, action: &ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Get { key } => get(&Session::open(ctx, global)?, key),
        ConfigAction::Explain { key } => explain(&Session::open(ctx, global)?, key),
        ConfigAction::Diff => diff(&Session::open(ctx, global)?),
        ConfigAction::Set { assignments } => set(Session::open(ctx, global)?, global, assignments),
        ConfigAction::Menu => menu(Session::open(ctx, global)?, global),
    }
}

/// A configured build directory and what Kconfig made of it.
struct Session {
    p: Prepared,
    menus: Menus,
    values: Values,
}

impl Session {
    fn open(ctx: &Ctx, global: &GlobalArgs) -> Result<Session> {
        let p = prepare(ctx, global, "config")?;
        let generated = build::generate(&p.project, &p.layout)?;
        let dir = p.layout.dir();
        let has_menus =
            dir.join("config.env").is_file() && dir.join("config/kconfig_menus.json").is_file();
        if has_menus && (generated.only_values || !generated.changed) {
            // The menus still hold, and kconfgen settles values in a fraction
            // of CMake's time. Ninja would not notice new defaults, so
            // without build.ninja the next build configures from scratch.
            let ninja = dir.join("build.ninja");
            if generated.changed && ninja.exists() {
                fsx::remove_file(&ninja)?;
            }
            let menus = Menus::load(&dir.join("config/kconfig_menus.json"))?;
            let values = if ninja.exists() {
                Values::load(&dir.join("config/sdkconfig.json"))?
            } else {
                let defaults = build::defaults_text(&p.project.manifest, &p.layout.profile);
                Evaluator::new(&p.build())?.eval(&defaults)?
            };
            return Ok(Session { p, menus, values });
        }
        p.build().configure(&generated)?;
        let (menus, values) = load(&p.layout)?;
        Ok(Session { p, menus, values })
    }

    fn evaluator(&self) -> Result<Evaluator> {
        Evaluator::new(&self.p.build())
    }

    fn defaults_text(&self) -> String {
        build::defaults_text(&self.p.project.manifest, &self.p.layout.profile)
    }

    fn config(&self) -> BTreeMap<ConfigKey, ConfigValue> {
        self.p
            .project
            .manifest
            .effective_config(&self.p.layout.profile)
    }

    /// Where idf.toml sets `key` for this profile, if it does.
    fn source(&self, key: &str) -> Option<String> {
        let m = &self.p.project.manifest;
        let profile = &self.p.layout.profile;
        let k: ConfigKey = key.parse().ok()?;
        if m.profile
            .get(profile)
            .is_some_and(|p| p.config.contains_key(&k))
        {
            Some(format!("idf.toml [profile.{profile}.config]"))
        } else if m.config.contains_key(&k) {
            Some("idf.toml [config]".to_owned())
        } else {
            None
        }
    }

    /// A Kconfig file location inside the SDK, as `esp-idf/components/…`.
    fn sdk_relative(&self, at: &str) -> String {
        let idf = self.p.installed.sdk.idf_path();
        let mut parts: Vec<&str> = Vec::new();
        for c in at.split('/') {
            match c {
                "." => {}
                ".." => {
                    parts.pop();
                }
                c => parts.push(c),
            }
        }
        let at = parts.join("/");
        match std::path::Path::new(&at).strip_prefix(&idf) {
            Ok(rel) => format!("esp-idf/{}", rel.display()),
            Err(_) => at,
        }
    }

    /// A key from the command line: a symbol or a choice.
    fn lookup(&self, key: &str) -> Result<Found<'_>> {
        let key = key.strip_prefix("CONFIG_").unwrap_or(key);
        if let Some(sym) = self.menus.settable(key) {
            return Ok(Found::Symbol(sym));
        }
        if let Some(choice) = self.menus.choices.get(key) {
            return Ok(Found::Choice(choice));
        }
        if let Some(sym) = self.menus.symbols.get(key) {
            return Ok(Found::Symbol(sym));
        }
        let suggestions = self.menus.suggest(key);
        if suggestions.is_empty() {
            return Err(miette::miette!(
                "{key} is not a Kconfig option for {}",
                self.p.layout.target
            ));
        }
        Err(miette::miette!(
            help = format!("Did you mean {}?", suggestions.join(", ")),
            "{key} is not a Kconfig option for {}",
            self.p.layout.target
        ))
    }
}

fn config_key(s: &str) -> Result<ConfigKey> {
    s.parse().map_err(|e: String| miette::miette!("{e}"))
}

enum Found<'a> {
    Symbol(&'a Symbol),
    Choice(&'a kconfig::Choice),
}

fn load(layout: &Layout) -> Result<(Menus, Values)> {
    let dir = layout.dir().join("config");
    Ok((
        Menus::load(&dir.join("kconfig_menus.json"))?,
        Values::load(&dir.join("sdkconfig.json"))?,
    ))
}

/// Warn about `[config]` entries the build ignores; called after configure.
pub fn check_build(p: &Prepared) -> Result<()> {
    let (menus, values) = load(&p.layout)?;
    let config = p.project.manifest.effective_config(&p.layout.profile);
    if kconfig::check(&config, &menus, &values).is_empty() {
        return Ok(());
    }
    let defaults = build::defaults_text(&p.project.manifest, &p.layout.profile);
    let ev = Evaluator::new(&p.build())?;
    for problem in ev.report(&defaults, &config, &menus, &values)? {
        ui::warn(format!("idf.toml: {problem}"));
    }
    Ok(())
}

/// A value for printing to stdout: bare strings, hex as `0x…`.
fn plain(v: &serde_json::Value, kind: Kind) -> String {
    match (v, kind) {
        (serde_json::Value::String(s), _) => s.clone(),
        (serde_json::Value::Number(n), Kind::Hex) => format!("{:#x}", n.as_i64().unwrap_or(0)),
        (other, _) => other.to_string(),
    }
}

fn inactive(sym: &Symbol, values: &Values) -> miette::Report {
    match &sym.depends_on {
        Some(dep) => miette::miette!(
            "{} is not active in this build: it depends on {}",
            sym.name,
            kconfig::explain_dependency(dep, values)
        ),
        None => miette::miette!("{} is not active in this build", sym.name),
    }
}

fn get(s: &Session, key: &str) -> Result<()> {
    match s.lookup(key)? {
        Found::Choice(c) => match s.values.selected(c) {
            Some(o) => println!("{o}"),
            None => return Err(miette::miette!("{} is not active in this build", c.name)),
        },
        Found::Symbol(sym) => match s.values.get(&sym.name) {
            Some(v) => println!("{}", plain(v, sym.kind)),
            None if sym.kind == Kind::Bool => println!("false"),
            None => return Err(inactive(sym, &s.values)),
        },
    }
    Ok(())
}

fn explain(s: &Session, key: &str) -> Result<()> {
    let ev = s.evaluator()?;
    let defaults = ev.defaults()?;
    let show = |v: Option<&serde_json::Value>, kind| match v {
        Some(v) => kconfig::toml_value(v, Some(kind)),
        None if kind == Kind::Bool => "false".to_owned(),
        None => "(inactive)".to_owned(),
    };
    let row = |label: &str, text: &str| {
        let label = if label.is_empty() {
            String::new()
        } else {
            format!("{label}:")
        };
        println!("  {label:<13}{text}");
    };
    let explain = |e: &str| kconfig::explain_dependency(e, &s.values);
    let menu = |path: &[String], title: &Option<String>| {
        let mut menu = path.to_vec();
        menu.extend(title.clone());
        if !menu.is_empty() {
            row("menu", &menu.join(" > "));
        }
    };
    let help = |help: &Option<String>| {
        if let Some(help) = help {
            println!();
            for line in help.lines() {
                println!("  {line}");
            }
        }
    };
    match s.lookup(key)? {
        Found::Symbol(sym) => {
            let why = ev.why(&s.defaults_text(), &[&sym.name])?.remove(&sym.name);
            let range = match (sym.range, sym.kind) {
                (Some((lo, hi)), Kind::Hex) => format!(", {lo:#x}..={hi:#x}"),
                (Some((lo, hi)), _) => format!(", {lo}..={hi}"),
                _ => String::new(),
            };
            println!("{}  ({}{range})", sym.name, sym.kind.name());
            menu(&sym.path, &sym.title);
            let now = s.values.get(&sym.name);
            let selected = why
                .as_ref()
                .is_some_and(|w| w.selected_by.iter().any(|t| t.active));
            let from = match (s.source(&sym.name), now) {
                (Some(src), _) => src,
                (None, None) if sym.kind != Kind::Bool => "inactive".to_owned(),
                _ if selected => "selected by another option".to_owned(),
                _ if now == defaults.get(&sym.name) => "Kconfig default".to_owned(),
                _ if sym.choice.is_some() => "chosen through other settings".to_owned(),
                _ => "follows from other settings".to_owned(),
            };
            row("value", &format!("{}  ({from})", show(now, sym.kind)));
            let mine = sym
                .name
                .parse::<ConfigKey>()
                .ok()
                .and_then(|k| s.config().get(&k).cloned());
            if let Some(v) = mine
                && !s.values.holds(&sym.name, &v)
            {
                let reason = kconfig::reason(sym, &v, &s.values, why.as_ref());
                row(
                    "",
                    &format!(
                        "idf.toml's {} has no effect: {reason}",
                        kconfig::show_config(&v)
                    ),
                );
            }
            row("default", &show(defaults.get(&sym.name), sym.kind));
            if let Some(c) = &sym.choice {
                row(
                    "choice",
                    &format!("{c} (set with `idf config set {c}=<OPTION>`)"),
                );
            }
            if sym.title.is_none() {
                row(
                    "set by",
                    "Kconfig only (no prompt, so idf.toml cannot set it)",
                );
            }
            let real = |e: &String| !e.starts_with("<choice");
            if let Some(dep) = why
                .as_ref()
                .and_then(|w| w.depends_on.as_ref())
                .or(sym.depends_on.as_ref())
                .filter(|e| real(e))
            {
                row("depends on", &explain(dep));
            }
            if let Some(w) = &why {
                for p in w
                    .prompts
                    .iter()
                    .filter(|p| p.cond.as_ref().is_some_and(real))
                {
                    let cond = p.cond.as_deref().unwrap_or_default();
                    // The dependency is part of every prompt condition; show
                    // what the prompt adds.
                    let extra = match depends_on_suffix(cond, w.depends_on.as_deref()) {
                        Some(e) => e,
                        None => cond.to_owned(),
                    };
                    if Some(extra.as_str()) != w.depends_on.as_deref() {
                        row("settable if", &explain(&extra));
                    }
                }
                let terms = |ts: &[kconfig::Term]| {
                    ts.iter()
                        .map(|t| format!("{}{}", t.expr, if t.active { " (active)" } else { "" }))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                if !w.selected_by.is_empty() {
                    row("selected by", &terms(&w.selected_by));
                }
                if !w.implied_by.is_empty() {
                    row("implied by", &terms(&w.implied_by));
                }
                print_defaults(&row, &w.defaults, w.depends_on.as_deref());
                for (i, at) in w.defined_at.iter().enumerate() {
                    row(if i == 0 { "defined at" } else { "" }, &s.sdk_relative(at));
                }
            }
            help(&sym.help);
        }
        Found::Choice(c) => {
            println!("{}  (choice)", c.name);
            menu(&c.path, &c.title);
            let selected = s.values.selected(c);
            let default = defaults.selected(c);
            let width = c.options.iter().map(String::len).max().unwrap_or(0);
            for o in &c.options {
                let mut marks = Vec::new();
                if Some(o.as_str()) == selected {
                    marks.extend(s.source(o));
                }
                if Some(o.as_str()) == default {
                    marks.push("default".to_owned());
                }
                let title = s
                    .menus
                    .symbols
                    .get(o)
                    .and_then(|o| o.title.clone())
                    .unwrap_or_default();
                let marks = if marks.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", marks.join(", "))
                };
                let star = if Some(o.as_str()) == selected {
                    "*"
                } else {
                    " "
                };
                println!("  {star} {o:<width$}  {title}{marks}");
            }
            if let Some(dep) = &c.depends_on {
                row("depends on", &explain(dep));
            }
            row("set with", &format!("idf config set {}=<OPTION>", c.name));
            let key = format!("choice:{}", c.name);
            if let Some(w) = ev.why(&s.defaults_text(), &[&key])?.remove(&key) {
                print_defaults(&row, &w.defaults, w.depends_on.as_deref());
                for (i, at) in w.defined_at.iter().enumerate() {
                    row(if i == 0 { "defined at" } else { "" }, &s.sdk_relative(at));
                }
            }
            help(&c.help);
        }
    }
    Ok(())
}

/// Conditional defaults, when there is more than one, marking the one that
/// applies (the first whose condition holds).
fn print_defaults(
    row: &dyn Fn(&str, &str),
    defaults: &[kconfig::Conditional],
    depends_on: Option<&str>,
) {
    if defaults.len() < 2 {
        return;
    }
    let first = defaults.iter().position(|d| d.active);
    for (i, d) in defaults.iter().enumerate() {
        let cond = d
            .cond
            .as_deref()
            .map(|c| match depends_on_suffix(c, depends_on) {
                Some(rest) => rest,
                None if Some(c) == depends_on => String::new(),
                None => c.to_owned(),
            });
        let cond = cond
            .filter(|c| !c.is_empty())
            .map(|c| format!(" if {c}"))
            .unwrap_or_default();
        let mark = if Some(i) == first { "  <- applies" } else { "" };
        row(
            if i == 0 { "defaults" } else { "" },
            &format!("{}{cond}{mark}", d.value),
        );
    }
}

/// `cond` without a trailing `&& <depends_on>` that Kconfig appends to
/// prompt conditions.
fn depends_on_suffix(cond: &str, depends_on: Option<&str>) -> Option<String> {
    let dep = depends_on?;
    let stripped = cond.strip_suffix(dep)?.trim_end().strip_suffix("&&")?;
    Some(stripped.trim().to_owned())
}

fn diff(s: &Session) -> Result<()> {
    let defaults = s.evaluator()?.defaults()?;
    let config = s.config();
    let mut own = Vec::new();
    let mut derived = Vec::new();
    for sym in s.menus.symbols.values().filter(|s| s.title.is_some()) {
        let k = &sym.name;
        let (now, was) = (s.values.get(k), defaults.get(k));
        let same = match sym.kind {
            Kind::Bool => s.values.is_y(k) == defaults.is_y(k),
            _ => now == was,
        };
        let Some(now) = now else { continue };
        if same {
            continue;
        }
        let comment = match (&sym.choice, was) {
            (Some(c), _) if s.values.is_y(k) => {
                match s.menus.choices.get(c).and_then(|c| defaults.selected(c)) {
                    Some(d) => format!("instead of {d}"),
                    None => "choice was inactive".to_owned(),
                }
            }
            // A choice option that was switched off: the selected one is listed.
            (Some(_), _) => continue,
            (None, Some(was)) => format!("default {}", kconfig::toml_value(was, Some(sym.kind))),
            (None, None) => "inactive by default".to_owned(),
        };
        let line = (
            format!("{k} = {}", kconfig::toml_value(now, Some(sym.kind))),
            comment,
        );
        if k.parse::<ConfigKey>()
            .is_ok_and(|k| config.contains_key(&k))
        {
            own.push(line);
        } else {
            derived.push(line);
        }
    }
    let width = own
        .iter()
        .chain(&derived)
        .map(|(l, _)| l.len())
        .max()
        .unwrap_or(0);
    let print = |lines: &[(String, String)]| {
        for (l, c) in lines {
            println!("{l:width$}  # {c}");
        }
    };
    println!(
        "# {} for {}: {} set in idf.toml, {} more that follow",
        s.p.layout.profile,
        s.p.layout.target,
        own.len(),
        derived.len()
    );
    print(&own);
    if !derived.is_empty() {
        println!("\n# Following from the settings above");
        print(&derived);
    }
    let unchanged: Vec<String> = config
        .iter()
        .filter(|(k, v)| defaults.holds(k.as_str(), v))
        .map(|(k, _)| k.to_string())
        .collect();
    if !unchanged.is_empty() {
        println!("\n# Already Kconfig's defaults: {}", unchanged.join(", "));
    }
    Ok(())
}

/// The profile table that edits go to: `[profile.<name>.config]` when the
/// profile was picked on the command line, `[config]` otherwise.
fn edit_profile(global: &GlobalArgs) -> Option<String> {
    (global.profile.is_some() || global.release).then(|| global.profile_name().to_owned())
}

fn table_name(profile: Option<&str>) -> String {
    match profile {
        Some(p) => format!("[profile.{p}.config]"),
        None => "[config]".to_owned(),
    }
}

/// idf.toml's form of a settled value.
fn config_value(v: &serde_json::Value, kind: Kind) -> Option<ConfigValue> {
    Some(match (kind, v) {
        (Kind::Bool, v) => ConfigValue::Bool(v.as_bool()?),
        (Kind::Int, v) => ConfigValue::Int(v.as_i64()?),
        (Kind::Hex, v) => ConfigValue::String(format!("{:#x}", v.as_i64()?)),
        (Kind::String, v) => ConfigValue::String(v.as_str()?.to_owned()),
    })
}

fn set(mut s: Session, global: &GlobalArgs, assignments: &[String]) -> Result<()> {
    let mut edits: Vec<(ConfigKey, Option<ConfigValue>)> = Vec::new();
    for a in assignments {
        let Some((key, text)) = a.split_once('=') else {
            return Err(miette::miette!(
                help = "Write KEY=VALUE, e.g. FREERTOS_HZ=1000.",
                "{a:?} is not KEY=VALUE"
            ));
        };
        let (sym, value) = match s.lookup(key.trim())? {
            Found::Choice(c) => {
                let Some(o) = kconfig::choice_option(c, text.trim()) else {
                    return Err(miette::miette!(
                        help = format!("Options: {}", c.options.join(", ")),
                        "{text:?} is not an option of {}",
                        c.name
                    ));
                };
                (&s.menus.symbols[o], ConfigValue::Bool(true))
            }
            Found::Symbol(sym) => {
                if sym.title.is_none() {
                    return Err(miette::miette!(
                        "{} has no prompt; Kconfig sets it from other options",
                        sym.name
                    ));
                }
                (sym, kconfig::parse_value(sym, text.trim())?)
            }
        };
        if let Some(c) = &sym.choice {
            if value == ConfigValue::Bool(false) {
                return Err(miette::miette!(
                    help = format!("Pick another option: idf config set {c}=<OPTION>"),
                    "{} is an option of the choice {c} and cannot be switched off alone",
                    sym.name
                ));
            }
            // One option per choice in idf.toml.
            for o in &s.menus.choices[c].options {
                if *o != sym.name {
                    edits.push((config_key(o)?, None));
                }
            }
        }
        edits.push((config_key(&sym.name)?, Some(value)));
    }
    let profile = edit_profile(global);
    let before = s.values.clone();
    s.p.project.edit_config(profile.as_deref(), &edits)?;
    let ev = s.evaluator()?;
    let after = ev.eval(&s.defaults_text())?;
    for (k, v) in edits.iter().filter_map(|(k, v)| Some((k, v.as_ref()?))) {
        let kind = s.menus.symbols[k.as_str()].kind;
        let was = match before.get(k.as_str()) {
            Some(w) if before.holds(k.as_str(), v) => {
                format!(" (was already {})", kconfig::toml_value(w, Some(kind)))
            }
            Some(w) => format!(" (was {})", kconfig::toml_value(w, Some(kind))),
            None => String::new(),
        };
        ui::status(
            "Set",
            format!(
                "{k} = {} in {}{was}",
                kconfig::show_config(v),
                table_name(profile.as_deref())
            ),
        );
    }
    let config = s.config();
    let set_keys: Vec<&ConfigKey> = edits
        .iter()
        .filter(|(_, v)| v.is_some())
        .map(|(k, _)| k)
        .collect();
    let mine: BTreeMap<ConfigKey, ConfigValue> = config
        .into_iter()
        .filter(|(k, _)| set_keys.contains(&k))
        .collect();
    for problem in ev.report(&s.defaults_text(), &mine, &s.menus, &after)? {
        ui::warn(problem);
    }
    Ok(())
}

/// The manifest with `edits` applied, without touching idf.toml.
fn with_edits(
    m: &crate::manifest::Manifest,
    profile: Option<&str>,
    edits: &[(ConfigKey, Option<ConfigValue>)],
) -> crate::manifest::Manifest {
    let mut m = m.clone();
    let table = match profile {
        Some(p) => &mut m.profile.entry(p.to_owned()).or_default().config,
        None => &mut m.config,
    };
    for (k, v) in edits {
        match v {
            Some(v) => table.insert(k.clone(), v.clone()),
            None => table.remove(k),
        };
    }
    m
}

fn menu(mut s: Session, global: &GlobalArgs) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(miette::miette!("menuconfig needs a terminal"));
    }
    let ev = s.evaluator()?;
    // menuconfig edits an sdkconfig made from idf.toml alone, the one a
    // build starts from. It sits in the build directory because menuconfig
    // shows its path.
    let work = s.p.layout.dir().join("menuconfig");
    fsx::create_dir_all(&work)?;
    let sdkconfig = work.join("sdkconfig");
    let before = ev.write_sdkconfig(&s.defaults_text(), &sdkconfig)?;
    ev.menuconfig(&sdkconfig)?;
    let after = ev.read(&sdkconfig)?;

    let profile = edit_profile(global);
    let table = match &profile {
        Some(p) => {
            s.p.project
                .manifest
                .profile
                .get(p)
                .map(|p| p.config.clone())
                .unwrap_or_default()
        }
        None => s.p.project.manifest.config.clone(),
    };
    let mut edits: Vec<(ConfigKey, Option<ConfigValue>)> = Vec::new();
    let mut changed = Vec::new();
    for sym in s.menus.symbols.values().filter(|s| s.title.is_some()) {
        let k = &sym.name;
        let same = match sym.kind {
            Kind::Bool => before.is_y(k) == after.is_y(k),
            _ => before.get(k) == after.get(k),
        };
        if same {
            continue;
        }
        let key = config_key(k)?;
        changed.push(sym);
        match (&sym.choice, after.get(k)) {
            // The deselected option of a choice: the selected one is written.
            (Some(_), _) if !after.is_y(k) => {
                if table.contains_key(&key) {
                    edits.push((key, None));
                }
            }
            (_, Some(v)) => edits.push((key, config_value(v, sym.kind))),
            // Switched off through a dependency; leave what idf.toml says.
            (_, None) => {}
        }
    }
    if edits.is_empty() {
        ui::status("Unchanged", "idf.toml (no options changed)");
        return Ok(());
    }

    // Entries set back to Kconfig's default can go, if the result stays the same.
    let defaults = ev.defaults()?;
    let trimmed: Vec<_> = edits
        .iter()
        .map(|(k, v)| match v {
            Some(v) if defaults.holds(k.as_str(), v) => (k.clone(), None),
            v => (k.clone(), v.clone()),
        })
        .collect();
    let holds_all = |values: &Values| {
        changed.iter().all(|sym| match sym.kind {
            Kind::Bool => values.is_y(&sym.name) == after.is_y(&sym.name),
            _ => values.get(&sym.name) == after.get(&sym.name),
        })
    };
    let layout = &s.p.layout;
    let eval = |edits: &[(ConfigKey, Option<ConfigValue>)]| {
        let m = with_edits(&s.p.project.manifest, profile.as_deref(), edits);
        ev.eval(&build::defaults_text(&m, &layout.profile))
    };
    let trimmed_result = if trimmed != edits {
        Some(eval(&trimmed)?).filter(|v| holds_all(v))
    } else {
        None
    };
    let (edits, result) = match trimmed_result {
        Some(v) => (trimmed, v),
        None => {
            let v = eval(&edits)?;
            (edits, v)
        }
    };
    // Removing what idf.toml does not have is no edit.
    let edits: Vec<_> = edits
        .into_iter()
        .filter(|(k, v)| v.is_some() || table.contains_key(k))
        .collect();
    if edits.is_empty() {
        ui::status("Unchanged", "idf.toml (no options changed)");
        return Ok(());
    }
    s.p.project.edit_config(profile.as_deref(), &edits)?;

    let table = table_name(profile.as_deref());
    for (k, v) in &edits {
        match v {
            Some(v) => ui::status(
                "Set",
                format!("{k} = {} in {table}", kconfig::show_config(v)),
            ),
            None => ui::status("Removed", format!("{k} from {table}")),
        }
    }
    for sym in &changed {
        let k = &sym.name;
        let ok = match sym.kind {
            Kind::Bool => result.is_y(k) == after.is_y(k),
            _ => result.get(k) == after.get(k),
        };
        if !ok {
            let show = |v: &Values| match v.get(k) {
                Some(x) => kconfig::toml_value(x, Some(sym.kind)),
                None => "inactive".to_owned(),
            };
            ui::warn(format!(
                "{k}: menuconfig chose {}, but idf.toml gives {}",
                show(&after),
                show(&result)
            ));
        }
    }
    Ok(())
}
