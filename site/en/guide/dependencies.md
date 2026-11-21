---
title: Dependencies
description: add, remove, update, tree; registry, git, and local path dependencies; conditions by chip and IDF version; [[component]] in idf.lock and --locked.
---

# Dependencies

Third-party components are declared in the `[dependencies]` table of `idf.toml`. `idf` translates them into the `idf_component.yml` of the generated `main` component. The IDF component manager, run from the SDK venv, resolves and downloads them. `managed_components/` and `dependencies.lock` are placed in `build/<target>/<profile>/project/`, not in the project directory. The resolution result is merged into `idf.lock`.

## Three sources

```toml
[dependencies]
# Registry: namespace/name = version requirement
"espressif/led_strip" = "^3.1.1"

# Registry, with conditions
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"] }

# git: optional rev (branch, tag, or commit) and subdir (subdirectory in the repository)
pid_ctrl = { git = "https://github.com/espressif/idf-extra-components.git", subdir = "pid_ctrl" }

# Local path: relative to the project root, or an absolute path
mylib = { path = "../mylib" }
```

- A string value is a version requirement for a registry dependency. It is passed to the component manager unchanged. The syntax is defined by the component manager.
- In table form, exactly one of `version`, `git`, or `path` must be given. `rev` and `subdir` can be used only together with `git`.
- A local path dependency must be named after the directory of the component, because the component manager looks up components by directory name.

### Conditional dependencies

```toml
[dependencies]
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"], idf = "6.0" }
```

- `targets = [...]`: the dependency is included only when building for these chips. The array cannot be empty.
- `idf = "6.0"` or `"6.0.3"`: the dependency is included only for this IDF series (or this exact version).

These are translated into component manager `rules`: `target in [esp32, esp32s3]` and `idf_version >=6.0.0,<6.1.0`. An exact version becomes `idf_version ==6.0.3`. `idf add` has no options for conditions. To add a condition, edit `idf.toml` directly.

## idf add

```bash
idf add espressif/led_strip                 # no requirement given
idf add espressif/led_strip@^3.1            # with a requirement
idf add espressif/led_strip espressif/button
idf add mylib --path ../mylib
```

A git dependency is added with `idf add NAME --git URL [--rev REV]`.

```text
   Resolving dependencies
      Adding espressif/led_strip ^3.1.1
      Locked 1 dependencies in idf.lock
```

- For a registry dependency with no requirement, resolution first runs with `*`. The requirement written to `idf.toml` is then set to `^<resolved version>`.
- `--git` and `--path` each accept only one dependency name. `--rev` must be used together with `--git`. `add` has no `subdir` option. When the component is in a subdirectory of the git repository (such as `pid_ctrl` above), write `subdir` in `idf.toml` directly.
- If the `--path` argument is inside the project, write it as a path relative to the project root. Otherwise, write an absolute path.

## idf remove

```bash
idf remove espressif/button
```

Removes the dependency from `idf.toml`, then resolves again. Components in `idf.lock` that are no longer needed are removed at the same time. If the name is not in `[dependencies]`, the command fails and lists the existing dependencies.

## idf update

```bash
idf update                      # all
idf update espressif/led_strip  # only these
```

Resolves again within the constraints in `idf.toml`. When names are given, only the locks for those dependencies, and for dependencies that were brought in only by them, are dropped. Other dependencies keep their current versions. At the end, the version change of each component is reported. If nothing changed, the output is `Unchanged everything is up to date`.

## idf tree

```bash
idf tree
```

```text
hello (ESP-IDF 6.0.3)
├── espressif/button 4.2.3
│   └── espressif/cmake_utilities 1.4.1
└── espressif/led_strip 3.1.1
```

Reads the resolution result from the last configuration. If there is none yet, resolves first. Under `--offline`, the command fails when no existing result is available and does not resolve. A component that appears more than once and has sub-dependencies is marked `(*)` and is not expanded again.

`add`, `remove`, `update`, and `tree` do not run CMake. They call the component manager's `prepare_components` directly.

## [[component]] in idf.lock

```toml
[[component]]
name = "espressif/led_strip"
version = "3.1.1"
source = "registry+https://components.espressif.com/"
hash = "sha256:027f68f2b57b0dca1d68dff6cc1139d9093825aa808c2bc3ca42bb2ce530fce9"
```

| Source | Form of `source` | Notes |
| --- | --- | --- |
| Registry | `registry+<URL>` | Includes `hash` (the component manager's `component_hash`) |
| git | `git+<URL>#<COMMIT>` | Pinned to a commit |
| Local path | `path+<PATH>` | Version is `*`. For a path inside the project, relative to the project root |

The lock takes effect as follows:

- **Registry**: a new build directory first writes a `dependencies.lock` from `idf.lock`, without `manifest_hash`. The component manager then resolves again, but starts from these versions, so the versions and hashes do not drift.
- **git**: the component manager re-resolves branches on every resolution, so `idf` writes the locked commit into the `version` of the generated manifest. The `rev` in `idf.toml` does not change.
- **Local path**: read from disk. No lock is needed.

The component manager's download cache is at `~/.idf-next/components/` (environment variable `IDF_COMPONENT_CACHE_PATH`) and is shared by all projects. When there are no dependencies, no manifest is written and the component manager is not run (`IDF_COMPONENT_MANAGER=0`), so configuration is not slowed down.

## --locked

```bash
idf --locked build
```

- `build`: after configuration, if the merged resolution would change `idf.lock`, the build fails. It suggests running `idf update` (or building without `--locked`) and committing `idf.lock`.
- `add`, `remove`, and `update` modify `idf.toml` and `idf.lock`. With `--locked`, they are rejected immediately:

```text
Error:   × `idf add` would change idf.toml and idf.lock, but --locked was given
```

When resolution fails, `idf` removes the NOTEs, progress dots, and "SDK is not a .git repository" warnings from the component manager's output. With `-v`, the full output is shown.

## Known limitations

::: warning Reconfigure after changing dependencies
After `idf add` in a directory that has already been built, the next `idf build` does not rerun the CMake configuration. The newly added component does not enter the build. (Observed: `build.ninja` does not contain the new component, and the build finishes in 0.2 seconds.) Deleting the build directory and building again works around this:

```bash
idf clean
idf build
```
:::

::: warning
- The first configuration of a new build directory still requires network access. Without `manifest_hash`, resolution is needed, and the component manager reads registry metadata. True offline reproduction requires a port of the resolver or computing `manifest_hash` yourself. Neither is implemented yet.
- `--offline` restricts only the network access of `idf` itself. It is not passed to the component manager.
- For `add`, `update`, and `tree`, resolution uses the `config/sdkconfig.json` left by the last configuration. If the project has never been configured, the file does not exist, and `matches` rules in the dependency manifest that depend on Kconfig values have no effect. After configuration, if `[config]` is changed, this file is stale until the next build.
- For dependencies with `targets` restrictions, `idf.lock` is the union across chips. Building for a different chip adds entries but does not remove the entries of other chips.
- The component manager's git cache has a problem with `file://` local repositories: new commits fail with `not a tree object`. Real https repositories are not affected.
:::
