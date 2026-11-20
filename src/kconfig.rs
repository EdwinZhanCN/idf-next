//! What Kconfig says about a build: every symbol's type, prompt, help and
//! dependencies (kconfgen's `json_menus` output), and the values it settles
//! on (`json`), for the `config` commands and the build's `[config]` check.
//!
//! Values come from running kconfgen the way IDF's CMake does, on defaults
//! text alone (no `sdkconfig`): that is what a build of the same `idf.toml`
//! produces, since `idf build` restarts `sdkconfig` whenever the defaults
//! change. It takes about 0.3 s and needs a configured build directory for
//! `config.env`.

use crate::build::{Build, Layout};
use crate::manifest::{ConfigKey, ConfigValue};
use miette::{IntoDiagnostic, Result, WrapErr};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Hex,
    String,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "bool" => Kind::Bool,
            "int" => Kind::Int,
            "hex" => Kind::Hex,
            "string" => Kind::String,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Bool => "bool",
            Kind::Int => "int",
            Kind::Hex => "hex",
            Kind::String => "string",
        }
    }
}

/// A config symbol as the menus show it.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    /// The prompt; symbols without one cannot be set by the user.
    pub title: Option<String>,
    pub help: Option<String>,
    pub range: Option<(i64, i64)>,
    pub depends_on: Option<String>,
    /// Titles of the enclosing menus.
    pub path: Vec<String>,
    /// The choice this symbol is an option of.
    pub choice: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub name: String,
    pub title: Option<String>,
    pub help: Option<String>,
    pub depends_on: Option<String>,
    pub path: Vec<String>,
    pub options: Vec<String>,
}

/// The menu tree, flattened.
#[derive(Debug, Default)]
pub struct Menus {
    pub symbols: BTreeMap<String, Symbol>,
    pub choices: BTreeMap<String, Choice>,
}

impl Menus {
    pub fn load(path: &Path) -> Result<Menus> {
        let text = crate::fsx::read_to_string(path)?;
        let items: Vec<serde_json::Value> = serde_json::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        let mut m = Menus::default();
        m.walk(&items, &mut Vec::new(), None);
        Ok(m)
    }

    fn walk(&mut self, items: &[serde_json::Value], path: &mut Vec<String>, choice: Option<&str>) {
        for it in items {
            let s = |k: &str| it.get(k).and_then(|v| v.as_str()).map(str::to_owned);
            let children = it
                .get("children")
                .and_then(|c| c.as_array())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let kind = s("type").unwrap_or_default();
            let name = s("name");
            match (kind.as_str(), name) {
                ("menu", _) => {
                    path.push(s("title").unwrap_or_default());
                    self.walk(children, path, None);
                    path.pop();
                }
                ("choice", Some(name)) => {
                    let options = children
                        .iter()
                        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                        .map(str::to_owned)
                        .collect();
                    self.choices.entry(name.clone()).or_insert(Choice {
                        name: name.clone(),
                        title: s("title"),
                        help: s("help"),
                        depends_on: s("depends_on"),
                        path: path.clone(),
                        options,
                    });
                    self.walk(children, path, Some(&name));
                }
                (k, Some(name)) => {
                    if let Some(kind) = Kind::parse(k) {
                        let range = it.get("range").and_then(|r| {
                            let r = r.as_array()?;
                            Some((int_of(r.first()?)?, int_of(r.get(1)?)?))
                        });
                        // A symbol defined in several places keeps the first
                        // definition that has a prompt.
                        let sym = Symbol {
                            name: name.clone(),
                            kind,
                            title: s("title"),
                            help: s("help"),
                            range,
                            depends_on: s("depends_on"),
                            path: path.clone(),
                            choice: choice.map(str::to_owned),
                        };
                        match self.symbols.get(&name) {
                            Some(old) if old.title.is_some() || sym.title.is_none() => {}
                            _ => {
                                self.symbols.insert(name, sym);
                            }
                        }
                    }
                    if !children.is_empty() {
                        self.walk(children, path, None);
                    }
                }
                _ => self.walk(children, path, None),
            }
        }
    }

    /// A user-settable symbol (it has a prompt).
    pub fn settable(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name).filter(|s| s.title.is_some())
    }

    /// Names close to `query`, for "did you mean".
    pub fn suggest(&self, query: &str) -> Vec<String> {
        let q = query.to_ascii_uppercase();
        let names = self.symbols.keys().chain(self.choices.keys());
        let mut scored: Vec<(usize, &String)> = names
            .filter_map(|n| {
                let d = edit_distance(&q, n);
                if d <= 2 || (q.len() >= 4 && n.contains(&q)) {
                    Some((d, n))
                } else {
                    None
                }
            })
            .collect();
        scored.sort();
        scored.dedup_by(|a, b| a.1 == b.1);
        scored.into_iter().take(5).map(|(_, n)| n.clone()).collect()
    }
}

fn int_of(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| {
        let s = v.as_str()?;
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => i64::from_str_radix(h, 16).ok(),
            None => s.parse().ok(),
        }
    })
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Settled values (`sdkconfig.json`): bools, integers (hex too) and
/// strings. Inactive symbols are absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values(pub BTreeMap<String, serde_json::Value>);

impl Values {
    pub fn load(path: &Path) -> Result<Values> {
        let text = crate::fsx::read_to_string(path)?;
        let map = serde_json::from_str(&text)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        Ok(Values(map))
    }

    pub fn get(&self, name: &str) -> Option<&serde_json::Value> {
        self.0.get(name)
    }

    /// Whether a bool symbol is `y` (absent means `n`).
    pub fn is_y(&self, name: &str) -> bool {
        self.get(name).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    /// The option a choice settled on.
    pub fn selected<'a>(&self, choice: &'a Choice) -> Option<&'a str> {
        choice
            .options
            .iter()
            .find(|o| self.is_y(o))
            .map(String::as_str)
    }

    /// Whether `value` (as idf.toml holds it) is what Kconfig settled on.
    pub fn holds(&self, key: &str, value: &ConfigValue) -> bool {
        match (self.get(key), value) {
            (None, ConfigValue::Bool(false)) => true,
            (Some(v), ConfigValue::Bool(b)) => v.as_bool() == Some(*b),
            (Some(v), ConfigValue::Int(n)) => v.as_i64() == Some(*n),
            (Some(v), ConfigValue::String(s)) => match v {
                serde_json::Value::String(t) => t == s,
                serde_json::Value::Number(n) => int_of(&serde_json::Value::String(s.clone()))
                    .is_some_and(|i| n.as_i64() == Some(i)),
                _ => false,
            },
            _ => false,
        }
    }
}

/// A value the way `idf.toml` writes it: `true`, `1000`, `"0x8000"`, `"text"`.
pub fn toml_value(v: &serde_json::Value, kind: Option<Kind>) -> String {
    match (v, kind) {
        (serde_json::Value::Number(n), Some(Kind::Hex)) => {
            format!("\"{:#x}\"", n.as_i64().unwrap_or(0))
        }
        (serde_json::Value::String(s), _) => {
            format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
        }
        (other, _) => other.to_string(),
    }
}

/// What the user typed for `KEY=VALUE`, checked against the symbol's type.
pub fn parse_value(sym: &Symbol, text: &str) -> Result<ConfigValue> {
    let name = &sym.name;
    let int = |t: &str| -> Option<i64> {
        match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(h) => i64::from_str_radix(h, 16).ok(),
            None => t.parse().ok(),
        }
    };
    let in_range = |n: i64| -> Result<()> {
        match sym.range {
            Some((lo, hi)) if n < lo || n > hi => {
                let show = |x: i64| match sym.kind {
                    Kind::Hex => format!("{x:#x}"),
                    _ => x.to_string(),
                };
                Err(miette::miette!(
                    "{name} must be between {} and {}",
                    show(lo),
                    show(hi)
                ))
            }
            _ => Ok(()),
        }
    };
    Ok(match sym.kind {
        Kind::Bool => match text.to_ascii_lowercase().as_str() {
            "true" | "y" | "yes" | "on" | "1" => ConfigValue::Bool(true),
            "false" | "n" | "no" | "off" | "0" => ConfigValue::Bool(false),
            _ => return Err(miette::miette!("{name} is a bool; use true or false")),
        },
        Kind::Int => {
            let Some(n) = int(text) else {
                return Err(miette::miette!(
                    "{name} is an int; {text:?} is not a number"
                ));
            };
            in_range(n)?;
            ConfigValue::Int(n)
        }
        Kind::Hex => {
            let Some(n) = int(text) else {
                return Err(miette::miette!(
                    "{name} is a hex value; {text:?} is not a number"
                ));
            };
            in_range(n)?;
            ConfigValue::String(format!("{n:#x}"))
        }
        Kind::String => ConfigValue::String(text.to_owned()),
    })
}

/// The option of `choice` that `text` names: the full option name, or the
/// part after the choice's name (`LOG_DEFAULT_LEVEL=DEBUG`).
pub fn choice_option<'a>(choice: &'a Choice, text: &str) -> Option<&'a str> {
    let t = text.to_ascii_uppercase();
    let prefixed = format!("{}_{t}", choice.name);
    choice
        .options
        .iter()
        .find(|o| **o == t || **o == prefixed)
        .map(String::as_str)
}

/// Why a symbol is not active: its dependency, with the current values of
/// the symbols it mentions, e.g. `ESP_CONSOLE_UART (= n)`.
pub fn explain_dependency(expr: &str, values: &Values) -> String {
    let re = regex::Regex::new(r"\b[A-Z][A-Z0-9_]+\b").expect("valid regex");
    let mut out = expr.to_owned();
    let mut seen = Vec::new();
    for m in re.find_iter(expr) {
        let name = m.as_str();
        if seen.contains(&name) || matches!(name, "y" | "n" | "m") {
            continue;
        }
        seen.push(name);
        let v = match values.get(name) {
            Some(serde_json::Value::Bool(true)) => "y".to_owned(),
            Some(serde_json::Value::Bool(false)) | None => "n".to_owned(),
            Some(v) => v.to_string(),
        };
        let _ = write!(
            out,
            "{}{name} = {v}",
            if seen.len() == 1 { " (" } else { ", " }
        );
    }
    if !seen.is_empty() {
        out.push(')');
    }
    out
}

/// Runs kconfgen and menuconfig against a configured build directory, with
/// the arguments IDF's `kconfig.cmake` passes.
pub struct Evaluator {
    python: PathBuf,
    idf_path: PathBuf,
    env: crate::sdk::Environment,
    layout: Layout,
    idf_version: String,
}

impl Evaluator {
    pub fn new(build: &Build<'_>) -> Result<Evaluator> {
        let installed = build.installed;
        if !build.layout.dir().join("config.env").is_file() {
            return Err(miette::miette!(
                "the build directory is not configured; run `idf build` first"
            ));
        }
        Ok(Evaluator {
            python: installed.python.python(),
            idf_path: installed.sdk.idf_path(),
            env: build.env().clone(),
            layout: build.layout.clone(),
            idf_version: installed.sdk.record.idf.version.to_string(),
        })
    }

    fn envs(&self) -> Vec<(String, String)> {
        let dir = self.layout.dir();
        vec![
            ("IDF_TARGET".into(), self.layout.target.to_string()),
            ("IDF_TOOLCHAIN".into(), "gcc".into()),
            ("IDF_ENV_FPGA".into(), String::new()),
            ("IDF_INIT_VERSION".into(), self.idf_version.clone()),
            ("IDF_MINIMAL_BUILD".into(), "n".into()),
            (
                "COMPONENT_KCONFIGS_SOURCE_FILE".into(),
                dir.join("kconfigs.in").display().to_string(),
            ),
            (
                "COMPONENT_KCONFIGS_PROJBUILD_SOURCE_FILE".into(),
                dir.join("kconfigs_projbuild.in").display().to_string(),
            ),
        ]
    }

    /// kconfgen on `config` (an sdkconfig, created if missing) and, when
    /// given, defaults text.
    fn kconfgen(&self, config: &Path, defaults: Option<&str>, write: bool) -> Result<Values> {
        let tmp = tempfile::tempdir().into_diagnostic()?;
        let json = tmp.path().join("sdkconfig.json");
        let mut cmd = std::process::Command::new(&self.python);
        self.env.apply(&mut cmd);
        cmd.args(["-m", "kconfgen", "--list-separator=semicolon", "--kconfig"])
            .arg(self.idf_path.join("Kconfig"))
            .arg("--sdkconfig-rename")
            .arg(self.idf_path.join("sdkconfig.rename"))
            .arg("--config")
            .arg(config);
        if let Some(text) = defaults {
            let path = tmp.path().join("sdkconfig.defaults");
            crate::fsx::write(&path, text)?;
            cmd.arg("--defaults").arg(path);
        }
        cmd.arg("--env-file")
            .arg(self.layout.dir().join("config.env"));
        for (k, v) in self.envs().into_iter().take(5) {
            cmd.arg("--env").arg(format!("{k}={v}"));
        }
        cmd.args([
            "--env",
            "KCONFIG_REPORT_VERBOSITY=quiet",
            "--output",
            "json",
        ])
        .arg(&json);
        if write {
            cmd.args(["--output", "config"]).arg(config);
        }
        crate::ui::detail("Running", format!("{cmd:?}"));
        let out = cmd
            .output()
            .into_diagnostic()
            .wrap_err("running kconfgen")?;
        if !out.status.success() {
            return Err(miette::miette!(
                "kconfgen failed:\n{}",
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
        Values::load(&json)
    }

    /// Settle `defaults` (sdkconfig.defaults text) from scratch, the way a
    /// build of it will.
    pub fn eval(&self, defaults: &str) -> Result<Values> {
        let tmp = tempfile::tempdir().into_diagnostic()?;
        self.kconfgen(&tmp.path().join("sdkconfig"), Some(defaults), false)
    }

    /// Write the sdkconfig a build of `defaults` starts from to `path`.
    pub fn write_sdkconfig(&self, defaults: &str, path: &Path) -> Result<Values> {
        if path.exists() {
            crate::fsx::remove_file(path)?;
        }
        self.kconfgen(path, Some(defaults), true)
    }

    /// The values an sdkconfig file holds, as Kconfig reads it.
    pub fn read(&self, sdkconfig: &Path) -> Result<Values> {
        self.kconfgen(sdkconfig, None, false)
    }

    /// Kconfig's own defaults for this target, with nothing from idf.toml.
    /// Cached in the build's config directory until its menus change.
    pub fn defaults(&self) -> Result<Values> {
        let config = self.layout.dir().join("config");
        let cache = config.join("idf-defaults.json");
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let (Some(c), Some(m)) = (
            modified(&cache),
            modified(&config.join("kconfig_menus.json")),
        ) && c >= m
        {
            return Values::load(&cache);
        }
        let values = self.eval("")?;
        let text = serde_json::to_string(&values.0).into_diagnostic()?;
        crate::fsx::write(&cache, text)?;
        Ok(values)
    }

    /// Why `names` settle as they do under `defaults`.
    pub fn why(&self, defaults: &str, names: &[&str]) -> Result<BTreeMap<String, Why>> {
        if names.is_empty() {
            return Ok(BTreeMap::new());
        }
        let tmp = tempfile::tempdir().into_diagnostic()?;
        let script = tmp.path().join("kconfig_why.py");
        crate::fsx::write(&script, include_str!("kconfig_why.py"))?;
        let defaults_path = tmp.path().join("sdkconfig.defaults");
        crate::fsx::write(&defaults_path, defaults)?;
        let output = tmp.path().join("why.json");
        let env: BTreeMap<String, String> = self.envs().into_iter().collect();
        let input = serde_json::json!({
            "kconfig": self.idf_path.join("Kconfig"),
            "rename": self.idf_path.join("sdkconfig.rename"),
            "env_file": self.layout.dir().join("config.env"),
            "env": env,
            "defaults": defaults_path,
            "names": names,
            "output": output,
        });
        let mut cmd = std::process::Command::new(&self.python);
        self.env.apply(&mut cmd);
        cmd.arg("-I")
            .arg(&script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().into_diagnostic().wrap_err("running Kconfig")?;
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("piped stdin");
            stdin
                .write_all(input.to_string().as_bytes())
                .into_diagnostic()?;
        }
        let out = child.wait_with_output().into_diagnostic()?;
        if !out.status.success() {
            return Err(miette::miette!(
                "reading Kconfig failed:\n{}",
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
        let text = crate::fsx::read_to_string(&output)?;
        serde_json::from_str(&text).into_diagnostic()
    }

    /// Problems with `config`, explained.
    pub fn report(
        &self,
        defaults: &str,
        config: &BTreeMap<ConfigKey, ConfigValue>,
        menus: &Menus,
        values: &Values,
    ) -> Result<Vec<String>> {
        let problems = check(config, menus, values);
        let whys = self.why(defaults, &no_effect_keys(&problems))?;
        Ok(describe(&problems, menus, values, &whys))
    }

    /// Upstream menuconfig on `sdkconfig`, in this terminal.
    pub fn menuconfig(&self, sdkconfig: &Path) -> Result<()> {
        let mut cmd = std::process::Command::new(&self.python);
        self.env.apply(&mut cmd);
        // `python -m esp_menuconfig`, minus the NOTE lines Kconfig prints
        // about the SDK's own files (deprecated hex literals and the like).
        const RUN: &str = "import runpy, sys\n\
            from esp_pylib.logger import log\n\
            log.note = lambda *a, **k: None\n\
            sys.argv = ['esp_menuconfig', sys.argv[1]]\n\
            runpy.run_module('esp_menuconfig', run_name='__main__', alter_sys=True)\n";
        cmd.envs(self.envs())
            .env("KCONFIG_CONFIG", sdkconfig)
            .arg("-c")
            .arg(RUN)
            .arg(self.idf_path.join("Kconfig"));
        crate::ui::detail("Running", format!("{cmd:?}"));
        let status = cmd
            .status()
            .into_diagnostic()
            .wrap_err("running menuconfig")?;
        if !status.success() {
            return Err(miette::miette!("menuconfig exited with {status}"));
        }
        Ok(())
    }
}

/// Why a symbol has its value, beyond what `kconfig_menus.json` records:
/// prompt conditions, `select`/`imply` sources and conditional defaults.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Why {
    pub visible: bool,
    pub prompts: Vec<Conditional>,
    pub depends_on: Option<String>,
    pub selected_by: Vec<Term>,
    pub implied_by: Vec<Term>,
    pub defaults: Vec<Conditional>,
    pub defined_at: Vec<String>,
}

/// A prompt or a default, with the `if` it carries.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Conditional {
    #[serde(alias = "text")]
    pub value: String,
    #[serde(rename = "if")]
    pub cond: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Term {
    pub expr: String,
    pub active: bool,
}

/// A `[config]` entry Kconfig does not take.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    /// Wrong as written: unknown key, a choice, a symbol without a prompt.
    Invalid(String),
    /// A valid entry whose value Kconfig did not settle on.
    NoEffect { key: String, value: ConfigValue },
}

/// Check `[config]` entries against what Kconfig settled on.
pub fn check(
    config: &BTreeMap<ConfigKey, ConfigValue>,
    menus: &Menus,
    values: &Values,
) -> Vec<Problem> {
    let mut out = Vec::new();
    for (key, value) in config {
        let k = key.as_str();
        if let Some(choice) = menus.choices.get(k)
            && menus.settable(k).is_none()
        {
            let all = || format!("set one of {} to true", choice.options.join(", "));
            let hint = match value {
                ConfigValue::String(s) => choice_option(choice, s)
                    .map(|o| format!("write `{o} = true`"))
                    .unwrap_or_else(all),
                _ => all(),
            };
            out.push(Problem::Invalid(format!(
                "{k} is a choice and cannot be assigned; {hint}"
            )));
            continue;
        }
        match menus.symbols.get(k) {
            None if values.get(k).is_some() => out.push(Problem::Invalid(format!(
                "{k} has no prompt, so Kconfig ignores values for it"
            ))),
            None => {
                let s = menus.suggest(k);
                let hint = if s.is_empty() {
                    String::new()
                } else {
                    format!(" (did you mean {}?)", s.join(", "))
                };
                out.push(Problem::Invalid(format!(
                    "{k} is not a Kconfig option of this build{hint}"
                )));
            }
            Some(sym) if sym.title.is_none() => out.push(Problem::Invalid(format!(
                "{k} has no prompt, so Kconfig ignores values for it"
            ))),
            Some(_) if values.holds(k, value) => {}
            Some(_) => out.push(Problem::NoEffect {
                key: k.to_owned(),
                value: value.clone(),
            }),
        }
    }
    out
}

/// The keys `why` should be asked about.
pub fn no_effect_keys(problems: &[Problem]) -> Vec<&str> {
    problems
        .iter()
        .filter_map(|p| match p {
            Problem::NoEffect { key, .. } => Some(key.as_str()),
            Problem::Invalid(_) => None,
        })
        .collect()
}

/// One line per problem, with the reason when `whys` has it.
pub fn describe(
    problems: &[Problem],
    menus: &Menus,
    values: &Values,
    whys: &BTreeMap<String, Why>,
) -> Vec<String> {
    problems
        .iter()
        .map(|p| match p {
            Problem::Invalid(msg) => msg.clone(),
            Problem::NoEffect { key, value } => {
                let sym = &menus.symbols[key.as_str()];
                let now = values
                    .get(key)
                    .map(|v| toml_value(v, Some(sym.kind)))
                    .unwrap_or_else(|| "inactive".to_owned());
                let why = reason(sym, value, values, whys.get(key));
                format!(
                    "{key} = {} has no effect ({now}): {why}",
                    show_config(value)
                )
            }
        })
        .collect()
}

/// Why `value` did not stick for `sym`.
pub fn reason(sym: &Symbol, value: &ConfigValue, values: &Values, why: Option<&Why>) -> String {
    let real = |e: &&String| !e.starts_with("<choice");
    let depends = why
        .and_then(|w| w.depends_on.as_ref())
        .or(sym.depends_on.as_ref())
        .filter(real);
    if let (None, Some(dep)) = (values.get(&sym.name), depends) {
        return format!("it depends on {}", explain_dependency(dep, values));
    }
    if let Some(w) = why {
        let selected: Vec<&str> = w
            .selected_by
            .iter()
            .filter(|t| t.active)
            .map(|t| t.expr.as_str())
            .collect();
        if !selected.is_empty() {
            return format!("selected by {}", selected.join(", "));
        }
        if !w.visible
            && let Some(cond) = w.prompts.iter().filter_map(|p| p.cond.as_ref()).find(real)
        {
            return format!(
                "it can only be set when {}",
                explain_dependency(cond, values)
            );
        }
        let implied: Vec<&str> = w
            .implied_by
            .iter()
            .filter(|t| t.active)
            .map(|t| t.expr.as_str())
            .collect();
        if !implied.is_empty() {
            return format!("implied by {}", implied.join(", "));
        }
    }
    if let (Some((lo, hi)), ConfigValue::Int(n)) = (sym.range, value)
        && (*n < lo || *n > hi)
    {
        return format!("outside the range {lo}..={hi}");
    }
    if let Some(c) = &sym.choice {
        return format!("another option of {c} is selected");
    }
    "Kconfig settled on another value".to_owned()
}

pub fn show_config(v: &ConfigValue) -> String {
    match v {
        ConfigValue::Bool(b) => b.to_string(),
        ConfigValue::Int(n) => n.to_string(),
        ConfigValue::String(s) => format!("{s:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn menus() -> Menus {
        let items = json!([
            {"type": "menu", "title": "Component config", "children": [
                {"type": "int", "name": "FREERTOS_HZ", "title": "Tick rate", "range": [1, 1000],
                 "help": "Hz", "depends_on": null},
                {"type": "bool", "name": "ESP_CONSOLE_UART", "title": "UART console"},
                {"type": "int", "name": "ESP_CONSOLE_UART_BAUDRATE", "title": "Baud",
                 "depends_on": "ESP_CONSOLE_UART"},
                {"type": "hex", "name": "PARTITION_TABLE_OFFSET", "title": "Offset"},
                {"type": "choice", "name": "LOG_DEFAULT_LEVEL", "title": "Level", "children": [
                    {"type": "bool", "name": "LOG_DEFAULT_LEVEL_INFO", "title": "Info"},
                    {"type": "bool", "name": "LOG_DEFAULT_LEVEL_DEBUG", "title": "Debug"}
                ]},
                {"type": "int", "name": "LOG_DEFAULT_LEVEL", "title": null}
            ]}
        ]);
        let mut m = Menus::default();
        m.walk(items.as_array().unwrap(), &mut Vec::new(), None);
        m
    }

    #[test]
    fn flattens_menus() {
        let m = menus();
        let hz = &m.symbols["FREERTOS_HZ"];
        assert_eq!(hz.range, Some((1, 1000)));
        assert_eq!(hz.path, ["Component config"]);
        assert_eq!(
            m.symbols["LOG_DEFAULT_LEVEL_DEBUG"].choice.as_deref(),
            Some("LOG_DEFAULT_LEVEL")
        );
        assert_eq!(m.choices["LOG_DEFAULT_LEVEL"].options.len(), 2);
        assert!(m.settable("LOG_DEFAULT_LEVEL").is_none());
        assert_eq!(m.suggest("FREERTOS_H"), ["FREERTOS_HZ"]);
    }

    #[test]
    fn parses_values_by_type() {
        let m = menus();
        let p = |k: &str, t: &str| parse_value(&m.symbols[k], t);
        assert_eq!(p("FREERTOS_HZ", "1000").unwrap(), ConfigValue::Int(1000));
        assert!(
            p("FREERTOS_HZ", "5000")
                .unwrap_err()
                .to_string()
                .contains("between 1 and 1000")
        );
        assert_eq!(p("ESP_CONSOLE_UART", "y").unwrap(), ConfigValue::Bool(true));
        assert_eq!(
            p("PARTITION_TABLE_OFFSET", "36864").unwrap(),
            ConfigValue::String("0x9000".into())
        );
        let c = &m.choices["LOG_DEFAULT_LEVEL"];
        assert_eq!(choice_option(c, "debug"), Some("LOG_DEFAULT_LEVEL_DEBUG"));
        assert_eq!(
            choice_option(c, "LOG_DEFAULT_LEVEL_INFO"),
            Some("LOG_DEFAULT_LEVEL_INFO")
        );
    }

    #[test]
    fn checks_config_against_settled_values() {
        let m = menus();
        let values = Values(
            [
                ("FREERTOS_HZ", json!(100)),
                ("ESP_CONSOLE_UART", json!(false)),
                ("PARTITION_TABLE_OFFSET", json!(0x9000)),
                ("LOG_DEFAULT_LEVEL", json!(3)),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
        );
        let config: BTreeMap<ConfigKey, ConfigValue> = [
            ("FREERTOS_HZ", ConfigValue::Int(5000)),
            ("ESP_CONSOLE_UART_BAUDRATE", ConfigValue::Int(921600)),
            (
                "PARTITION_TABLE_OFFSET",
                ConfigValue::String("0x9000".into()),
            ),
            ("LOG_DEFAULT_LEVEL", ConfigValue::String("DEBUG".into())),
            ("FREERTOS_HX", ConfigValue::Int(1)),
            ("ESP_CONSOLE_UART", ConfigValue::Bool(false)),
        ]
        .into_iter()
        .map(|(k, v)| (k.parse().unwrap(), v))
        .collect();
        let problems = check(&config, &m, &values);
        assert_eq!(
            no_effect_keys(&problems),
            ["ESP_CONSOLE_UART_BAUDRATE", "FREERTOS_HZ"]
        );
        let hidden: Why = serde_json::from_value(json!({
            "visible": false,
            "prompts": [{"text": "Tick rate", "if": "FREERTOS_CUSTOM", "active": false}],
            "depends_on": null, "selected_by": [], "implied_by": [], "defaults": [],
            "defined_at": []
        }))
        .unwrap();
        let whys: BTreeMap<String, Why> = [("FREERTOS_HZ".to_owned(), hidden)].into();
        assert_eq!(
            describe(&problems, &m, &values, &BTreeMap::new()),
            [
                "ESP_CONSOLE_UART_BAUDRATE = 921600 has no effect (inactive): it depends on ESP_CONSOLE_UART (ESP_CONSOLE_UART = n)",
                "FREERTOS_HX is not a Kconfig option of this build (did you mean FREERTOS_HZ?)",
                "FREERTOS_HZ = 5000 has no effect (100): outside the range 1..=1000",
                "LOG_DEFAULT_LEVEL is a choice and cannot be assigned; write `LOG_DEFAULT_LEVEL_DEBUG = true`",
            ]
        );
        assert_eq!(
            describe(&problems, &m, &values, &whys)[2],
            "FREERTOS_HZ = 5000 has no effect (100): it can only be set when FREERTOS_CUSTOM (FREERTOS_CUSTOM = n)"
        );
    }

    #[test]
    fn formats_values_as_toml() {
        assert_eq!(toml_value(&json!(32768), Some(Kind::Hex)), "\"0x8000\"");
        assert_eq!(toml_value(&json!(100), Some(Kind::Int)), "100");
        assert_eq!(toml_value(&json!("dio"), None), "\"dio\"");
        assert_eq!(toml_value(&json!(true), None), "true");
    }
}
