---
title: Roadmap and status
description: What each phase of idf-next (P0–P5) completed, how its gate was verified, on which hardware it was verified, and the known limitations and open items.
---

# Roadmap and status

idf-next is implemented in six phases, targeting ESP-IDF v6.0.x. P0 through P5 are all complete. After P5, the plan is no longer pursued in sequence. A phase is only ported when needed (see [Design principles](/en/design/principles#when-to-port)).

This page is taken from the implementation notes in `docs/PLAN.md`.

## Overview

| Phase | Content | Completion date | Gate |
| --- | --- | --- | --- |
| P0 | Skeleton and distribution | 2026-11-13 | After `uv tool install idf-next`, `idf --version` reports the tool version and the supported IDF range |
| P1 | SDK and toolchain | 2026-11-13 | After the cache is deleted, the same environment, including the venv, can be reproduced offline from the lock |
| P2a | Build loop | 2026-11-13 | The examples bundled with IDF build, and the project directory contains no extra files |
| P2b | Device loop | 2026-11-19 | Daily use works on a board |
| P3 | Configuration | 2026-11-20 | Changing configuration only requires editing `idf.toml` |
| P4 | Dependencies | 2026-11-20 | `managed_components` and `dependencies.lock` do not appear in the project directory |
| P5 | Long-tail thin wrappers | 2026-11-20 | No need to write by hand arguments that `idf` already knows for upstream commands |

## Verification environment

- **Host**: macOS arm64 (M-series Mac). The release process builds wheels for three platforms: macOS arm64, Linux x86_64 and Linux aarch64. The platform verified locally is macOS arm64.
- **SDK**: ESP-IDF v6.0.3.
- **Board**: ESP32-P4, chip revision v1.3, USB-Serial-JTAG, 16 MB flash. This is the only device verified on hardware.

::: warning Limited hardware coverage
All device-related verification was done on this one ESP32-P4 (USB-Serial-JTAG). Boards with UART bridge chips (CP210x, CH34x), FTDI debuggers, and `flash`, `monitor`, `debug` and `efuse` on real chips other than the P4 have not been tested on hardware.
:::

## P0 Skeleton and distribution

Completed:

- The supported range is written in `src/idf_support.rs`: `SUPPORTED_IDF_RANGE = ">=6.0.0, <6.1.0"`. `idf --version` outputs the tool version and this range. `-V` outputs only one line with the version.
- The schemas of `idf.toml` and `idf.lock` (`src/manifest.rs`, `src/lockfile.rs`). All errors point to the line and column through miette.
- The complete command tree can already be parsed. `completions` works.
- Distribution: `pyproject.toml` packages the tool with maturin using `bindings = "bin"`. The only Python dependency is `uv`. `.github/workflows/release.yml` builds wheels for three platforms, and publishes to PyPI when a `v*` tag is pushed.

Gate verification: on the local macOS arm64 machine, the wheel installed with `uv tool install` prints the correct output for `idf --version`, and `uv` and `idf` are installed in the same bin directory. CI installs the wheel on macOS arm64 and Linux x86_64 and checks `idf --version`. The Linux aarch64 wheel is cross-compiled, and CI cannot run it.

## P1 SDK and toolchain

Completed:

- Global cache layout (`~/.idf-next/`), file lock, `IDF_NEXT_HOME`.
- IDF source: only the non-`.git` entries of the release zip are downloaded using HTTP Range requests (about 130 MB, while the whole zip is about 2 GB). Each entry is verified with CRC-32, and the digest of the source tree is written to `idf.lock`.
- Toolchains: reads the SDK's `tools.json`, installs the `install: always` tools plus `cmake` and `ninja`, and downloads and extracts with 4 parallel threads. The lock records all archives for three platforms.
- Python: resolved with the bundled uv using a fixed `--exclude-newer`. The venv is named after the digest of the resolution. Only PyPI is used. The default interpreter is 3.12.
- Commands: `sdk install | use | list | remove | path`, `env [--shell]`, `doctor`; `--offline`, `--locked`.
- Supports the `IDF_GITHUB_ASSETS` mirror and `GITHUB_TOKEN`.

Gate verification (macOS arm64, ESP-IDF v6.0.3): after `idf sdk remove 6.0.3`, running `idf --offline --locked sdk install` rebuilt an SDK (source, toolchains and venv, 50,559 files in total) that is byte-identical to the one before removal. `idf.lock` is unchanged.

## P2a Build loop

Completed: `new`, `init`, `target`, `build`, `clean`, `native`. A CMake project is generated under `build/<target>/<profile>/`. Incremental builds work. `compile_commands.json` is linked into the project root.

Gate verification: first, the 458 examples bundled with ESP-IDF v6.0.3 were counted:

- 192 depend on the component manager, which belongs to P4;
- 63 have custom logic in CMake (`EMBED_FILES`, target-specific branches, cmakev2, etc.);
- The remaining 195 can be converted directly to the `idf.toml` layout.

A conversion script turned these 195 into the `idf.toml` layout. 33 were sampled by category and built with `idf --offline --locked build`, covering esp32, esp32s3, esp32c6, esp32h2 and esp32p4. All passed.

- The first round exposed a problem: `rsource` in `Kconfig.projbuild` looks up files by relative path, which fails after the file is copied to the generated directory. It is now handled by placing a stub with `source "<absolute path>"` in the generated directory.
- `security/nvs_encryption_hmac` requires `NVS_SEC_HMAC_EFUSE_KEY_ID = 0` in `[config]`. The upstream example itself also requires this value to be set manually.
- After all examples were built, the project directories contained only `idf.toml`, `idf.lock`, `src/`, `components/` and the `compile_commands.json` link.

A complete build of one example takes about 25 seconds. Building again without code changes takes about 0.1 seconds (M-series Mac).

## P2b Device loop

Completed: `flash`, `monitor` (with backtrace decoding), `run`, `erase`, `image`. Automatic serial port detection by VID/PID. espflash 4.7.0 (pinned version, with the `cli` feature enabled only to reuse `ResolvingPrinter`; `rust-version` was raised to 1.95 as a result).

Gate verification: on an ESP32-P4 v1.3 (USB-Serial-JTAG, 16 MB flash, macOS arm64), the everyday use of `run`, `flash`, `monitor`, `erase` and `image` all works.

- When the P4 panics, the monitor restores the stack dump to a full call chain with source line numbers.
- The three formats of `image` are byte-identical to the output of `idf native merge-bin / uf2 / dfu`. DFU is also identical with `ESP_DFU_PART_SIZE=0x10000` chunks.
- Reset timing: only espflash resets once at the end of writing. `run` now resets after the monitor opens the port, which avoids a second reset on the P4 (see [Architecture](/en/design/architecture#run-flow)).

Follow-up additions (2026-11-20):

- **Unplug and reconnect** has been tested on the P4. After `idf run`, unplugging the USB cable causes the monitor to print `disconnected; waiting for it`. When the cable is plugged back in, it prints `is back` and reattaches automatically to the port with the same name, without any manual action. When the port reappears, the first lines of ROM and bootloader output have already been printed, so the output after reconnection starts near `cpu_start`.
- **User config** `~/.config/idf/config.toml` (`port`, `flash_baud`): the port and priority have been verified. `flash_baud` was confirmed on the board. When it is set to 115200, there is no espflash high-baud-rate warning. With the default 460800 and with `--baud` override, the warning is shown.

## P3 Configuration

Completed: `[config]` generates `sdkconfig.defaults`. `config get | set | diff | explain | menu`. Build-time checks of `[config]`. The `config` command skips the CMake configuration run unless the defaults or partition table have changed.

Gate verification: on the hello project, `config set` and `config menu` were used to change the partition table offset (0x9000), flash size (16 MB), tick rate, log level, and the release optimization options. After building and flashing to the P4 board, the boot log shows the partition table at 0x9000 and flash at 16 MB, and the application runs normally. The project directory still contains only `idf.toml`, `idf.lock` and `src/`.

## P4 Dependencies

Completed: `add | remove | update | tree`. `[dependencies]` is translated into the generated `idf_component.yml`. The component manager's results are merged into `idf.lock`. New build directories are pre-seeded from the lock. Component download caches are shared in `~/.idf-next/components/`.

Gate verification: `managed_components` and `dependencies.lock` do not appear in the project directory. The resolver does not need to be ported, because the component manager treats the generated `build/<target>/<profile>/project/` as the project directory. On the hello project, the build used all three kinds of dependencies: registry (`espressif/led_strip`, `espressif/button`), git (the `pid_ctrl` subdirectory of `idf-extra-components`) and local path. After flashing to the P4 board, the application called a function from the path dependency and printed the expected output.

## P5 Long-tail thin wrappers

Completed: `size`, `secure`, `coredump`, `efuse`, `debug`. Each is `python -m <tool>` (`debug` is `idf.py openocd gdb`) with the arguments filled in. Exit codes are passed back unchanged.

Gate verification:

| Command | Verification |
| --- | --- |
| `size` | Compared line by line with `idf native size`. The content of `--format json2` is identical. |
| `secure` | Generated, signed and verified keys offline. The results are the same as calling `espsecure` directly. |
| `coredump` | Only the command assembly was verified. No real dump was decoded. |
| `efuse` | With `--virt`, `summary` is byte-identical to the direct call, and virtual burning works. On real chips, only the read-only `summary` was run, on the P4. No eFuse was burned. |
| `debug` | On the P4 v1.3 built-in USB-Serial-JTAG, `board/esp32p4-builtin.cfg` is used automatically. With `gdb_memory_map disable` added, GDB can connect, and the `app_main` breakpoint uses a hardware breakpoint. |

The lists of burn commands and read-only commands for the eFuse gate are written by hand. `tests/efuse_commands.rs` reads the "Burn commands" and "Read commands" sections of `--help` from the installed `espefuse` for 12 chips. It requires that every burn command is in the burn list and every read command is in the read-only list. It also fails if an unknown group appears. (The test was confirmed to fail by temporarily removing one entry.) When no SDK is installed, the test is skipped.

These five commands do not carry over Espressif's tests. They only call upstream, and upstream's tests cover upstream's internal logic.

## Known limitations and open items

### Device

- `monitor` has no log file output or timestamps, and no Ctrl+T menu like esp-idf-monitor.
- Only verified on USB-Serial-JTAG. The reset timing for UART bridges (CP210x, CH34x) follows espflash's and has not been tested on hardware (no device with a bridge chip is available).
- When `idf native … monitor` is killed, the `idf.py` child process does not exit with it. (Interactive Ctrl+C is sent to the whole process group, so it is not affected.)

### Build

- The `EMBED_FILES` / `EMBED_TXTFILES` settings of the `main` component and custom CMake logic cannot be expressed in `idf.toml`.
- Running `idf.py` in the managed environment first prints a "not a git repository" warning, because the SDK has no `.git`.
- Running upstream Python scripts writes `__pycache__` into the shared SDK directory. The SDK directory is therefore no longer read-only after use.
- The bootloader sub-project runs CMake again by itself after sdkconfig changes. Its output leaks into the output of `idf build` (`idf.py` also behaves this way).

### Configuration

- `config set` only checks the value being set. Other values that change as a side effect of enabling an option are currently only visible in `config diff`.
- For symbols whose prompt is hidden (for example `ESP_CONSOLE_UART_BAUDRATE`, which depends on `ESP_CONSOLE_UART_CUSTOM`), `set` still writes the value and only gives a warning, because the user may be about to enable the prerequisite at the same time.

### Dependencies

- The first configuration of a new build directory still needs network access. Without `manifest_hash`, resolution is required, and the component manager needs to read registry metadata. For truly offline reproduction, either the resolver must be ported, or `manifest_hash` must be computed by `idf`.
- `add`, `update` and `tree` pass the `config/sdkconfig.json` left by the last configuration to the component manager. If the project has never been configured, this file does not exist, and manifest `matches` rules that depend on Kconfig values have no effect. After a configuration, if `[config]` is changed, this file is stale until the next build.
- With dependencies that have `targets` restrictions, `idf.lock` is the union across chips. A build for another chip only adds entries and does not remove the entries for other chips.
- The component manager's git cache has a problem with `file://` local repositories (new commits fail with `not a tree object`). Real https repositories are not affected. This was not investigated further.

### Debugging and security

- `debug` has only been tried on USB-Serial-JTAG. FTDI or bridged boards still use the SDK's default OpenOCD arguments, and need `OPENOCD_COMMANDS` to be specified manually. The handling of `gdb_memory_map disable` has only been verified on the P4 v1.3.
- `coredump` has not been tested with a real dump.
- `efuse` has only been run on a real P4 chip, with the read-only `summary`. Other chips are covered only by `--virt`. `secure` does not touch the chip.

### Platforms and distribution

- Only macOS arm64, Linux x86_64 and Linux aarch64 are supported. On other hosts (including Windows), `idf` reports `unsupported host`.
- The terminal logo uses the Espressif mark, which is a trademark. It is used according to the brand guidelines in the press kit. This needs to be confirmed before the wheel is published to PyPI.

## Risks and open questions

| Risk | Status |
| --- | --- |
| SDK venv cannot be reproduced | Mitigated, verified by the P1 gate |
| Tool and SDK versions are incompatible | Mitigated: the supported range is in the code, `doctor` checks it, and anything outside it is an error |
| Environment check by `native` | Resolved: `IDF_TOOLS_PATH` and `IDF_PYTHON_ENV_PATH` point to the managed venv |
| Location of `managed_components` | Resolved, verified by the P4 gate |
| Parsing upstream tool output | Followed consistently: pass-through only, JSON for structured data |
| v6.0 Build System v2 is a preview | Still based on v1. Will be evaluated once v2 is stable. |
| eFuse burning is irreversible | Delegated to `espefuse`, with a second confirmation. Never burned on a real chip. |

Open questions:

- Should configuration items of user-defined components be written in `idf.toml` and translated into Kconfig by the tool, or should components be allowed to keep their own Kconfig files?
- Which chips are supported by default? The size of the test matrix depends on this answer.

## Later

Only when a delegated tool is slow, has ugly output, or is frequently broken is it ported, with differential tests to ensure consistency. The most likely candidates:

- **Kconfig evaluation**: so that the `config` command does not have to wait for CMake configuration.
- **Dependency resolver**: so that third-party components can enter the global cache, and so that a new build directory can be configured offline.
