---
title: Design principles
description: The goals and invariants of idf-next, the trade-off between delegating to upstream and native implementation, the reasons for the build directory layout, and risks and mitigations.
---

# Design principles

This page summarizes the design decisions in `docs/PLAN.md`. For each one, it describes how far it is implemented in the current code.

## Goals

idf-next is a personal ESP-IDF toolchain: a single Rust binary, `idf`. The repository contains only `idf.toml`, `idf.lock` and the source code. High-frequency everyday paths are implemented natively in Rust. The rest is delegated to upstream Python tools. The Python environment is managed by `idf`, and the user does not see it.

Two explicit non-goals:

- **No compatibility with old projects**. There is no import mode for projects without a `CMakeLists.txt`, and `sdkconfig` in the project directory is not read.
- **No tracking of the upstream release cadence**. Only ESP-IDF `>=6.0.0, <6.1.0` is supported. The range is written in the code (`src/idf_support.rs`).

## Invariants

Any implementation decision must not break the nine invariants below.

| # | Invariant | Status |
| --- | --- | --- |
| 1 | **The project directory** contains only `idf.toml`, `idf.lock`, `src/`, and optionally `components/`. It must not contain `CMakeLists.txt`, `sdkconfig`, `idf_component.yml`, `dependencies.lock` or `managed_components/`. | Holds. All upstream files are generated under `build/`. In addition, the project root may contain `build/`, a symbolic link to the `compile_commands.json` of the latest build, and the `.gitignore` written by `idf new` (which ignores the first two). |
| 2 | **External dependencies**: the ESP-IDF SDK source, CMake, Ninja, the cross GCC, and the Python venv for that SDK version. Debugging additionally needs OpenOCD and GDB. | Holds. Everything is installed into the global cache by `idf sdk install`. It does not depend on the CMake and Ninja installed on the system. |
| 3 | **Tool and SDK are decoupled**: idf-next has its own version number, and the code declares the supported IDF range. Python dependencies belong to the SDK and are not bundled with the tool. | Holds. `idf --version` reports the supported range. `idf.toml`, `idf.lock`, `idf sdk install` and `idf doctor` all check against this range. |
| 4 | **Python is not visible**: the venv for each SDK version is created by the uv bundled with the tool, and is installed under `~/.idf-next/sdk/<version>/`. No environment needs to be activated, and `export.sh` is not needed. | Holds. Every command that invokes upstream tools sets up its environment itself. `idf env` is only for calling tools outside `idf`. |
| 5 | **Boundary between native and delegated**: high-frequency, latency-sensitive features and features whose output must look good are implemented natively. Low-frequency, high-risk or large features are delegated to upstream. `idf native` can pass through to `idf.py` at any time. | Holds. See below. |
| 6 | **The SDK is not patched**: IDF's CMake framework and build-time scripts run as they are. | Holds. The generated `CMakeLists.txt` includes IDF's `project.cmake` unchanged. The `version.txt` added to the source is not a patch: IDF reads it anyway when there is no `.git`. |
| 7 | **Single version pinning**: `idf.lock` pins the IDF commit, toolchain hashes, Python version and the hash of the dependency resolution, independently of the tool version. Upgrades are explicit actions. | Holds. See [idf.lock](/en/reference/idf-lock). |
| 8 | **Build outputs are isolated**: all generated files are placed in `build/`. The SDK, its toolchains and venv are placed in the global cache. | Mostly holds. One exception: running upstream Python scripts writes `__pycache__` into the shared SDK directory. This does not affect correctness, but the SDK directory is no longer read-only after use. |
| 9 | **Definition of correctness**: for the parts implemented natively, the output must be byte-identical to the upstream tool they replace. | Differential verification was done for `image`: its three formats are byte-identical to `idf native merge-bin / uf2 / dfu`. SDK reinstallation offline is byte-identical to the state before removal. |

### CLI decisions that follow from the invariants

- **There is no global `--idf-version`**. Allowing the SDK version to change on every invocation would make `idf.lock` meaningless. The version is specified in only two places: `[project] idf` in `idf.toml` (written by `idf new`), and `idf sdk use <version>`.
- **`[project] idf` only accepts `"6.0"` or `"6.0.1"`**, not semver constraints. The project tracks one release series, and the exact version is pinned by the lock.
- **`-t/--target` is a one-time override, and `idf target set` writes to `idf.toml`**. The command-line option takes precedence over `idf.toml`.
- **The serial port does not go into `idf.toml`**. The port depends on the machine. It comes from `-p`, `IDF_PORT` or `~/.config/idf/config.toml`, and is auto-detected from USB VID/PID by default.
- **`--locked` exists alongside `--offline`**. The former forbids rewriting the lock, the latter forbids network access. Both are needed to guarantee reproducibility.
- **`new <path>` and `init` are separate**, following Cargo's semantics.
- **`native` is an escape hatch**. It runs `idf.py` in the managed environment, with the same build directory as `idf build`, so both operate on the same project.

## Delegation to upstream and native implementation

### Criteria

- **Native implementation**: high-frequency, latency-sensitive, and output must look good. The daily loop (build, flash, monitor) runs many times a day. Startup overhead and output quality are directly noticeable.
- **Delegate to upstream**: low-frequency, high-risk or large. A mistake with `espefuse` can permanently damage hardware. The component manager, Kconfig evaluation and ldgen are large and evolve with IDF. Reimplementing them would mean tracking upstream for the long term.
- **When delegating, only write a thin wrapper**: `idf` only fills in arguments and passes output through. It does not parse human-readable text. When structured data is needed, use upstream's JSON output (`json` and `json_menus` from kconfgen, `esp-idf-size --format json2`, and the component manager's `dependencies.lock` converted to JSON with ruamel.yaml from the SDK venv). This way, a change in upstream's display format does not break `idf`.

### Current state

| Feature | Strategy | Implementation |
| --- | --- | --- |
| SDK, toolchain and venv installation | Native | Reads `tools.json` and downloads and extracts by itself. For the IDF source, only the non-`.git` entries of the release zip are downloaded using HTTP Range requests. The venv is created with the bundled uv. |
| Build driver | Native orchestration, build itself delegated | `idf` generates the CMake project and invokes CMake and Ninja. IDF's CMake framework and build-time scripts (kconfgen, ldgen, etc.) run unchanged in the managed venv. |
| Flashing | Native | espflash 4.7.0 (pinned to `=4.7.0`). Writes according to `flasher_args.json`, plus esptool's pre-write checks. |
| Serial monitor | Native | The serial loop, key handling and reconnection are implemented by hand. Address and stack backtrace decoding reuses `ResolvingPrinter` from espflash. |
| Erase | Native | espflash. The partition table is parsed with esp-idf-part. |
| Image export (merge-bin, UF2, DFU) | Native | Small in size. Implemented by hand, and byte-identical to upstream. |
| Debug orchestration | Delegated | `idf.py openocd gdb`. `idf` only adds OpenOCD arguments when it detects the built-in USB-Serial-JTAG. |
| Kconfig evaluation | Delegated | kconfgen. The logic of `config get / set / diff / explain` is `idf`'s own. Its data comes from kconfgen's JSON output. An additional small script, `kconfig_why.py`, embedded in the binary, explains information missing from the JSON (prompt conditions, sources of select / imply, defaults with conditions). |
| menuconfig | Delegated | Upstream `esp_menuconfig`. On exit, the differences are written back to `idf.toml`. |
| Component management | Delegated resolution | The upstream component manager resolves and downloads. `idf` translates `[dependencies]` into a manifest, merges the results into `idf.lock`, and pre-seeds new build directories from the lock. |
| size, coredump, eFuse, security | Delegated | `python -m <tool>`, with chip, port, ELF and map paths filled in. Exit codes are passed back unchanged. |

### When to port {#when-to-port}

After P5, the plan is no longer pursued in sequence. A delegated tool is ported only when it is really in the way (slow, ugly output, or frequently broken), and differential tests are used to ensure it matches upstream. The two most likely candidates:

- **Kconfig evaluation**: so that the `config` command does not have to wait for CMake configuration. It already avoids CMake as much as possible (only when defaults or the partition table change, kconfgen is called directly, taking about 0.3 seconds). The first time, however, configuration still has to run once (about 3 seconds).
- **Dependency resolver**: so that third-party components can enter the global cache, and so that configuring a new build directory offline becomes possible.

eFuse and security tools are explicitly not to be ported. The risk is high and the frequency is low, so delegating to upstream and adding confirmation is safer.

## Build directory layout

```text
build/
  <target>/
    <profile>/                 CMake binary dir (sdkconfig, build.ninja, outputs)
      project/                 generated CMake source dir, the component manager's "project dir"
        CMakeLists.txt
        main/CMakeLists.txt
        main/idf_component.yml generated when there are dependencies
        sdkconfig.defaults
        partitions.csv         generated when [[partition]] exists
        dependencies.lock      written by component manager
        managed_components/    written by component manager
      menuconfig/              idf config menu working dir
      config/                  kconfgen JSON output
```

Why the layout is like this:

- **Isolated by chip** (`build/<target>/`): switching chips does not trigger a full reconfiguration. A one-time chip change with `-t` does not break the original build.
- **Isolated by profile** (`<profile>/`): `dev` and `release` each have their own sdkconfig and outputs. Switching back and forth is an incremental build.
- **The profile directory is itself the CMake binary directory**, and the generated source directory `project/` is placed inside it. Everything from one build is in one directory, and `idf clean` removes it cleanly.
- **The component manager treats `project/` as the project directory**, so `managed_components/` and `dependencies.lock` naturally end up in the build directory. P4 originally worried that these two could not be moved out of the project directory, which would force an own implementation of the resolver. This layout made the P4 gate hold without porting the resolver.
- **`idf native` and `idf debug` use the same pair of directories** (`-C build/<t>/<p>/project -B build/<t>/<p>`), so they operate on the same project as `idf build`.
- **`sdkconfig` exists only in the build directory**. When `sdkconfig.defaults` changes, `sdkconfig` is deleted and regenerated, because IDF defaults only fill in missing values, and `idf.toml` is the only source of truth.

Incremental build rules: a generated file whose content has not changed is not rewritten, and Ninja decides by itself what needs recompiling. CMake configuration is run again only if any generated file has changed or there is no `build.ninja` yet. Building again without code changes takes about 0.1 seconds (on an M-series Mac).

## Global cache and versions

- `~/.idf-next/sdk/<version>/` holds all contents of one SDK (source, toolchains, venv). They are installed and removed together.
- Downloads are stored by hash. uv's cache reuses package files by hard link, so multiple SDK versions side by side take up almost no extra space. An SDK can also be reinstalled offline after removal.
- Installing and removing an SDK holds the file lock `~/.idf-next/.lock`. If several `idf` processes install at the same time, the later ones wait.
- The tool itself is managed by `uv tool install` / `uv tool upgrade`, independently of the SDK. Upgrading the tool does not touch the SDK, and switching the SDK does not require reinstalling the tool. The project refers to a specific SDK version only through `idf.lock`.

## Risks and mitigations

After delegating to upstream, the main risks changed from "is the port correct" to two things: whether the managed environment can be reproduced, and whether the files that upstream writes into the project directory can be moved into `build/`.

| Risk | Impact | Mitigation | Status |
| --- | --- | --- | --- |
| SDK venv cannot be reproduced | Upstream pip dependencies drift | Resolve with the bundled uv, using that version's constraints and a fixed `--exclude-newer`. The hash of the resolution is written to `idf.lock`. | P1 verified: byte-identical after removal and offline reinstall |
| Tool and SDK versions are incompatible | Output of a new tool does not work with an old SDK | The supported range is declared in the code. `doctor` checks it, and anything outside it is an immediate error. | Implemented |
| Environment check by `native` | `idf.py` validates the Python environment at startup and may refuse to run | Point environment variables at the SDK venv | Resolved: `IDF_TOOLS_PATH` and `IDF_PYTHON_ENV_PATH` |
| Location of `managed_components` | It may not be possible to move it out of the project directory | If not possible, port the resolver | Resolved: the generated project directory is the component manager's project directory |
| Parsing upstream tool output | Breaks as soon as upstream changes the format | Only pass arguments and output through. Use JSON when structured data is needed. | Followed |
| v6.0 Build System v2 is a preview | A later version may change the CMake entry point | Stay on v1 for now. Evaluate v2 after it is stable. | Based on v1 |
| eFuse burning is irreversible | Permanent hardware damage | Delegate to `espefuse`. `idf` only adds an extra confirmation. | On real chips, burning requires typing `burn` manually, and `--do-not-confirm` is refused. The list of burn commands is checked by a test against `espefuse --help`. |

## Open questions

- Should configuration items of user-defined components be written in `idf.toml` and translated into Kconfig by the tool, or should components be allowed to keep their own Kconfig files? Currently `src/Kconfig`, `src/Kconfig.projbuild`, and each component's own Kconfig files under `components/` are all usable.
- Which chips are supported by default? The size of the test matrix depends on this answer. Currently any name of the form `esp32*` is accepted. The known list is the non-preview chips of ESP-IDF v6.0.
