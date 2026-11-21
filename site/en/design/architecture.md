---
title: Architecture
description: The source modules of idf-next, the data flow of one idf build and idf run, and the relation between the managed environment and idf.lock.
---

<ArchitectureDiagram />

# Architecture

`idf` is the only entry point. It reads `idf.toml` and `idf.lock` from the project directory, installs the SDK, toolchains and Python venv into the global cache at `~/.idf-next/`, translates `idf.toml` into a CMake project under `build/`, and then builds with the CMake and Ninja that come with the SDK. IDF's CMake framework runs unchanged. The Python scripts it calls during the build run in the venv managed by `idf`. Flashing, monitoring and image export are implemented natively by `idf`. The remaining low-frequency features are delegated to upstream tools.

## Source modules

Each file under `src/` is one module.

### Entry point and command line

| File | Responsibility |
| --- | --- |
| `main.rs` | Parses the command line and calls `lib.rs`. Errors and help from clap are output through `banner.rs`. |
| `lib.rs` | Loads the user config (fills `port` into the global options), sets the output level, and dispatches to `commands/` by command. |
| `cli.rs` | The command tree and global options defined with clap, and the help templates grouped as "Project / Build and device / Debugging and security / Environment". A test ensures that the templates list all commands. |
| `banner.rs` | Draws the logo to the left of help or version information, only for `idf`, `idf --help` and `idf --version`, and only when stdout is a terminal. |
| `logo.rs` | Rasterizes the embedded 512×512 bitmap into Braille characters, following the real character cell proportions of the terminal, with a gradient color. Output is byte-identical to the Python reference implementation in the press kit. |
| `ui.rs` | Status lines, warnings, progress bars and output levels (`-q`, `-v`) on stderr. Handles `NO_COLOR`. |

### Commands (`commands/`)

| File | Responsibility |
| --- | --- |
| `commands/build.rs` | `build`, `clean`, `native`. `prepare` is the common step for all commands that work on a build directory: find the project, check the profile, ensure the SDK, determine the layout. `record_dependencies` merges the component manager's results into `idf.lock`. |
| `commands/new.rs` | `new`, `init`, `target set`, `target list`. |
| `commands/config.rs` | `config get / set / diff / explain / menu`, and the check of `[config]` after a build. |
| `commands/deps.rs` | `add`, `remove`, `update`, `tree`. |
| `commands/device.rs` | `flash`, `run`, `monitor`, `erase`, `image`, including the checks before writing and the reset timing. |
| `commands/tools.rs` | The five thin wrappers `size`, `efuse`, `secure`, `coredump`, `debug`, and the confirmation for eFuse burning. |
| `commands/sdk.rs` | `sdk install / list / use / remove / path`. `ensure_project` decides whether to install from the lock or resolve a new version and write the lock. |
| `commands/env.rs` | `env`, `env --shell`. |
| `commands/doctor.rs` | Read-only checks for `doctor`. |

### Project files

| File | Responsibility |
| --- | --- |
| `manifest.rs` | Schema and validation of `idf.toml`: config keys, dependencies, size formats for partitions, profiles. |
| `lockfile.rs` | Schema of `idf.lock`, format version check, deterministic serialization and atomic writes. |
| `project.rs` | Searches upward from the current directory for the project. Edits `idf.toml` in place with toml_edit (preserving comments). The edited result is validated before it is written to disk. |
| `diagnostic.rs` | miette diagnostics for file errors, pointing to the line and column where the error occurred. |
| `chip.rs` | Validation of chip names and the list of known chips. |
| `idf_support.rs` | The supported ESP-IDF range (`>=6.0.0, <6.1.0`), the default version `6.0` written by `idf new`, and the parsing and matching of `[project] idf`. |
| `user_config.rs` | `~/.config/idf/config.toml` (`port`, `flash_baud`). |

### SDK and managed environment

| File | Responsibility |
| --- | --- |
| `home.rs` | Directory layout of the global cache `~/.idf-next/`. Can be overridden by `IDF_NEXT_HOME`. |
| `platform.rs` | Name of the host platform (`macos-arm64`, `linux-amd64`, `linux-arm64`), matching the naming in `tools.json`. |
| `sdk.rs` | Resolves ESP-IDF release versions (GitHub API), installs the SDK (source, toolchains and venv), generates lock entries, and builds the environment variables needed by upstream tools. |
| `idf_source.rs` | Reads the central directory of the release zip. Downloads only the non-`.git` entries using HTTP Range requests (about 130 MB, while the whole zip is about 2 GB). Verifies CRC-32, computes the digest of the source tree, and writes `version.txt`. |
| `tools_json.rs` | Reads the SDK's `tools/tools.json`, applies `platform_overrides` for the host platform, selects the tools marked `install: always` plus `cmake` and `ninja`, and computes `export_paths` and `export_vars`. |
| `python.rs` | Locates the bundled uv, resolves Python dependencies with `uv pip compile`, and creates the venv named after the digest of the resolution. The resolution result and constraints are stored in the download cache, keyed by digest. |
| `net.rs` | HTTP: downloads with retries, verification by sha256 and size, Range requests, GitHub API (`GITHUB_TOKEN`), the `IDF_GITHUB_ASSETS` mirror, and the `--offline` check. |
| `archive.rs` | Extracts `.tar.gz`, `.tar.xz` and `.zip`. Extracts to a temporary directory first, then renames, so a half-extracted tool is never left behind. |
| `fsx.rs` | File operation wrappers whose error messages include the path. |

### Build and configuration

| File | Responsibility |
| --- | --- |
| `build.rs` | Translates `idf.toml` into a CMake project under `build/<target>/<profile>/project/` (rewrites only when content changes), runs CMake configuration and `cmake --build`, and links `compile_commands.json`. |
| `deps.rs` | Translates `[dependencies]` into `idf_component.yml`. Reads the component manager's `dependencies.lock`. Keeps it in sync with `idf.lock`. Calls the component manager's `prepare_components` directly to resolve. |
| `kconfig.rs` | Reads `kconfig_menus.json` and `sdkconfig.json` from kconfgen. Runs kconfgen directly with the parameters from IDF's `kconfig.cmake` to compute values. Parses and checks the values for `config set`. |
| `kconfig_why.py` | A small script embedded in the binary. It is written to a temporary directory at runtime and run with `python -I` from the SDK venv. It answers questions that are not in `kconfig_menus.json`: prompt conditions, sources of select / imply, and defaults with conditions. |

### Devices

| File | Responsibility |
| --- | --- |
| `firmware.rs` | Reads build outputs: images and offsets from `flasher_args.json`, values from `sdkconfig`, the chip ID and version range from the image header. Selects the ROM ELF and the application ELF. |
| `serial.rs` | Serial port enumeration and selection by USB VID/PID. Entering ROM download mode. Reset (releases RTS first, then DTR). Flashing progress. |
| `monitor.rs` | Serial monitor. The serial loop, key handling, and disconnect and reconnect are implemented here. Address and stack backtrace decoding is delegated to espflash's `ResolvingPrinter`. |
| `image.rs` | Three image formats, `merge-bin` (raw), UF2 and DFU, byte-identical to upstream. |

## Data flow of one `idf build` {#build-flow}

```text
 project                          idf                                    global cache ~/.idf-next/
 ─────────                        ───                                    ─────────────────────
 idf.toml ──┐
 idf.lock ──┼──▶ project::discover  ──▶ commands::sdk::ensure_project ──▶ sdk/<version>/
 src/       │    manifest check         · lock current: install per lock   esp-idf/   (source)
 components/┘                           · else: resolve/install/write lock tools/     (CMake, Ninja, GCC…)
                                                    │                      python/<digest>/venv
                                                    ▼
                                   build::generate (write only on content change)
                                                    │
                       build/<target>/<profile>/project/
                         CMakeLists.txt         ← [project], components/, SDKCONFIG path
                         main/CMakeLists.txt    ← all source files under src/
                         main/Kconfig*          ← source stub for src/Kconfig*
                         sdkconfig.defaults     ← [config] + [profile.<p>.config] + partition switch
                         partitions.csv         ← [[partition]]
                         main/idf_component.yml ← [dependencies] (when there are dependencies)
                         dependencies.lock      ← seeded from idf.lock (fresh build dir)
                                                    │
                     defaults changed: rm sdkconfig │ generated files changed or no build.ninja
                                                    ▼
                    cmake -G Ninja -S project -B build/<t>/<p> -DPYTHON=<venv python>
                      IDF's project.cmake runs unchanged:
                      kconfgen → sdkconfig, config/*.json
                      component manager (IDF_COMPONENT_MANAGER=1) → managed_components/, dependencies.lock
                                                    │
                                                    ▼
                    deps::read_solved → deps::merge ──▶ idf.lock [[component]] (write only on change)
                    config::check_build             ──▶ [config] warnings
                                                    │
                                                    ▼
                    cmake --build (Ninja; ldgen etc. run in venv)
                                                    │
                                                    ▼
                    build/<t>/<p>/  <name>.elf  <name>.bin  bootloader/  partition_table/
                                    flasher_args.json  project_description.json
                    project root    compile_commands.json → build/<t>/<p>/compile_commands.json
```

Key points:

1. **The SDK is ready first**. `ensure_project` checks whether `idf.lock` exists and matches the `idf` key in `idf.toml`. If it matches, it installs exactly what the lock specifies (using the installed copy if it is already there). Otherwise it resolves the latest matching version, installs it, and writes the lock. See [idf.lock](/en/reference/idf-lock).
2. **Generation is idempotent**. Each generated file is rewritten only when its content changes, so Ninja's own dependency tracking decides what to recompile. If nothing has changed and `build.ninja` already exists, CMake configuration is skipped, and building again takes about 0.1 seconds.
3. **sdkconfig is regenerated from `idf.toml`**. When `sdkconfig.defaults` changes, `sdkconfig` is deleted, so that old values do not remain.
4. **CMake configuration output is collapsed**. It is shown only on failure or with `-v`. Ninja's output is passed through directly.
5. **Component dependencies enter `idf.lock` through the component manager** (next section).
6. **Configuration checks** run only after a real reconfiguration. They read kconfgen's JSON and warn about `[config]` entries that have no effect.

### How dependencies enter idf.lock {#deps-flow}

```text
idf.toml [dependencies]
        │  deps::component_manifest
        ▼
project/main/idf_component.yml        targets → rules: if target in [...]
        │                             idf     → rules: if idf_version ...
        │                             git dep's version ← commit pinned in idf.lock
        │
        │   fresh build dir: deps::sync_files writes dependencies.lock from idf.lock (no manifest_hash)
        ▼
component manager (CMake configure, or add/update/tree calling prepare_components directly)
        │  resolve; download to ~/.idf-next/components/ (IDF_COMPONENT_CACHE_PATH)
        ▼
project/dependencies.lock, project/managed_components/
        │  deps::read_solved (ruamel.yaml in venv, to JSON)
        ▼
idf.lock [[component]]  name / version / source / hash
```

- The component manager treats the generated `project/` as the project directory. As a result, `managed_components/` and `dependencies.lock` stay in the build directory and never appear in the project directory.
- The `dependencies.lock` pre-seeded in a new build directory has no `manifest_hash`. The component manager resolves again, but starts from the versions in the lock. The versions and hashes of registry dependencies therefore do not drift.
- The `deps.stamp` file in the build directory records the most recent merge. If `dependencies.lock` has not changed, the merge is not repeated.
- When there are no dependencies, no manifest is written, and `IDF_COMPONENT_MANAGER=0` is set, so the component manager does not run.

## Data flow of one `idf run` {#run-flow}

```text
idf run
  │
  ├─ commands::build::build          same as idf build; outputs flasher_args.json and images
  │
  ├─ flash_built (after = NoResetNoStub)
  │    serial::select_port           -p / IDF_PORT / config.toml port / USB VID/PID probe
  │    serial::connect               flash baud: -b / flash_baud / 460800
  │    device_info                   chip revision, flash size
  │    preflight                     image header chip ID/revision range, flash overflow
  │                                  (on failure, reset to old firmware)
  │    write_bins_to_flash           per flasher_args.json; pad to 4 bytes; skip same-MD5 images
  │                                  no reset after write
  │
  └─ monitor::run
       open port (release RTS, then DTR, so opening does not reset)
       set baud: sdkconfig ESP_CONSOLE_UART_BAUDRATE (-b has no effect here)
       reset once
       loop: serial out → espflash ResolvingPrinter (app ELF + ROM ELF) → terminal
             keys → serial; Ctrl+R reset; Ctrl+C / Ctrl+] exit
             port gone → resume when same-named port returns
```

Reset happens only once, after the monitor has opened the port. The reasons:

- At the end of writing, espflash resets once according to the connection's `ResetAfterOperation`. If the board is reset again after flashing, the second reset interrupts the startup that has just begun. On the ESP32-P4's USB-Serial-JTAG, this shows up as another `CHIP_USB_UART_RESET` near `app_main` during startup. With repeated resets, the ROM can even panic.
- For this reason, `run` flashes with `NoResetNoStub`. The monitor opens the port, sets the baud rate, and then resets. Boards with a UART bridge also do not lose the first lines of output.
- The ROM ELF is selected according to the chip revision read during flashing. It takes the largest version number under `ESP_ROM_ELF_DIR` that does not exceed the chip revision. This is the same approach as esp-idf-monitor.

`idf flash` on its own uses `HardReset`. After writing, espflash resets once. `idf monitor` on its own does not build and does not install the SDK. The chip revision is taken from `ESP_REV_MIN_FULL` in sdkconfig.

## Managed environment and idf.lock {#managed-env}

```text
idf.lock                         ~/.idf-next/
────────                         ─────────────
[idf] version ─────────────────▶ sdk/<version>/
      commit                       sdk.toml          installed [idf] and tools; written last
      source  ── check ──────────▶ esp-idf/          release zip non-.git entries + version.txt
                                 downloads/esp-idf/esp-idf-<tag>.zip  sparse zip, required entries only
[[tool]] archives.<host> ─ check ▶ tools/<name>/<version>/
                                 downloads/sha256-<hex>/<file>   archives stored by hash
[python] version ──────────────▶ python/             uv-managed interpreter
         exclude-newer ┐
         constraints  ─┼─ resolve ─▶ downloads/constraints/, downloads/python/ (stored by digest)
         requirements ─┘          sdk/<version>/python/<first 16 of requirements>/venv
[[component]] ─────────────────▶ components/          component manager download cache
```

- **One directory per SDK version**. Source, toolchains and venv are installed and removed together. `sdk.toml` and each venv's `env.toml` are written last, as the marker that installation is complete. An interrupted installation is shown as incomplete by `idf sdk list`, and the next installation completes it.
- **One SDK version can have multiple venvs**, distinguished by the digest of the resolution result. Two projects that pin different Python resolutions do not overwrite each other.
- **The lock is the only reference**. A project refers to a specific SDK version, toolchain hashes and Python resolution through `idf.lock`. Anything not in the lock is not used.
- **Offline reinstallation**. `idf sdk remove` only deletes `sdk/<version>/`. `downloads/`, `python/` and `uv-cache/` are kept, so `idf --offline --locked sdk install` can rebuild a byte-identical SDK from the lock.
- **Upstream tools see a standard environment**. `IDF_PATH`, `IDF_TOOLS_PATH`, `IDF_PYTHON_ENV_PATH` and `PATH` point to the managed directories, and CMake configuration passes `-DPYTHON=<venv python>`. IDF's own `check-python-dependencies` therefore passes unchanged, and `idf.py` also runs normally under `idf native`. For the list of variables, see [Environment variables and user config](/en/reference/environment#upstream).
