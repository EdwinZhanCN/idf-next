use crate::cli::GlobalArgs;
use crate::commands::sdk::ensure_project;
use crate::project::Project;
use crate::sdk::{Ctx, Environment};
use miette::Result;

pub fn run(ctx: &Ctx, global: &GlobalArgs, shell: bool) -> Result<()> {
    let mut project = Project::require(global.project.as_deref(), "env")?;
    let installed = ensure_project(ctx, &mut project, global.locked)?;
    let env = installed.environment();
    print!(
        "{}",
        if shell {
            render_shell(&env)
        } else {
            render_plain(&env)
        }
    );
    Ok(())
}

/// POSIX shell exports, for `eval "$(idf env --shell)"`.
pub fn render_shell(env: &Environment) -> String {
    let mut out = String::new();
    for (k, v) in &env.vars {
        out.push_str(&format!("export {k}={}\n", quote(v)));
    }
    let dirs: Vec<String> = env
        .path
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    out.push_str(&format!(
        "export PATH={}\"${{PATH}}\"\n",
        quote(&format!("{}:", dirs.join(":")))
    ));
    out
}

fn render_plain(env: &Environment) -> String {
    let width = env
        .vars
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let mut out = String::new();
    for (k, v) in &env.vars {
        out.push_str(&format!("{k:<width$}  {v}\n"));
    }
    for (i, p) in env.path.iter().enumerate() {
        let label = if i == 0 { "PATH" } else { "" };
        out.push_str(&format!("{label:<width$}  {}\n", p.display()));
    }
    out
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn shell_output_is_quoted() {
        let env = Environment {
            vars: vec![("IDF_PATH".into(), "/a b/it's".into())],
            path: vec![PathBuf::from("/x/bin"), PathBuf::from("/y")],
        };
        assert_eq!(
            render_shell(&env),
            "export IDF_PATH='/a b/it'\\''s'\nexport PATH='/x/bin:/y:'\"${PATH}\"\n"
        );
    }
}
