---
title: Quick start
description: Install idf, create the first project, build, flash, and open the serial monitor.
---

# Quick start

This page starts with installation and walks through `new`, `build`, and `run`.

## Prerequisites

- **Host platform**: macOS arm64, Linux x86_64, or Linux aarch64. On other platforms, `idf` fails immediately with `unsupported host`.
- **uv**: `idf` uses uv to create a Python environment for each SDK version. It looks for uv in this order: the environment variable `IDF_NEXT_UV`, a `uv` in the same directory as the real path of `idf`, then `uv` on `PATH`.
- **ESP-IDF version**: this version of `idf` supports ESP-IDF `>=6.0.0, <6.1.0`. You do not need to download the SDK yourself; see [SDK and environment](/en/guide/sdk).

## Installation

Installing from source requires Rust 1.95 or later:

```bash
cargo install --git https://github.com/EdwinZhanCN/idf-next
```

::: tip Packaging
The repository already sets up maturin (`bindings = "bin"`), which packages `idf` as a platform wheel. The only Python dependency is `uv`, so with `uv tool install idf-next`, `uv` and `idf` are installed into the same bin directory. The release workflow publishes the wheel to PyPI when a `v*` tag is pushed. Until then, install from source as shown above.
:::

Check the version and the supported ESP-IDF range:

```bash
idf --version
```

```text
idf 0.1.0
supported ESP-IDF: >=6.0.0, <6.1.0
```

`idf -V` prints only the first line. When you run `idf --version` in a terminal, it also draws the Espressif logo on the left. The logo is not drawn when the output is piped, when `TERM=dumb` is set, or when `-q` is given.

## Create a project

```bash
idf new blink -t esp32s3
cd blink
```

```text
     Created blink for esp32s3 (ESP-IDF 6.0) in blink
```

`idf new <path>` requires that the directory does not exist. To set up a project in an existing directory, use `idf init`, which does not overwrite an existing `src/`. If you omit `-t`, the target chip defaults to `esp32`. The project name is taken from the directory name. It must start with a letter and may contain only letters, digits, `-`, and `_`.

The generated project has three files:

```text
blink/
├── .gitignore      # /build/ and /compile_commands.json
├── idf.toml
└── src/
    └── main.c
```

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"
```

`idf = "6.0"` means the latest release in the 6.0 series. The first time an SDK is needed, `idf` writes the exact resolved version to `idf.lock`. For a complete description of `idf.toml`, see [Project and idf.toml](/en/guide/project).

## Build

```bash
idf build
```

The first build resolves and installs the SDK (IDF source, toolchains, Python environment) before it compiles, and it needs network access. Output looks similar to this:

```text
      Locked ESP-IDF 6.0.3 in idf.lock
 Configuring dev for esp32s3
    Building blink for esp32s3 (dev)
    ...
    Finished build/esp32s3/dev/blink.bin (160.41 KiB) in 28.8s
```

Output from the CMake configure step is hidden. It is shown only when the step fails or when `-v` is given. Ninja output is passed through directly. Building again without code changes takes about 0.1 seconds:

```text
    Finished build/esp32s3/dev/blink.bin (160.41 KiB) in 0.1s
```

After the build, the project root contains `idf.lock`, `build/`, and a symbolic link `compile_commands.json` that points to this build, for tools such as clangd. Commit `idf.lock` to version control.

## Check the environment {#check-env}

```bash
idf doctor
```

```text
Tool
  ✓ idf-next 0.1.0 (supports ESP-IDF >=6.0.0, <6.1.0)
  ✓ host platform macos-arm64
  ✓ uv 0.12.1 (...)
  ✓ SDK cache ~/.idf-next
Project (/path/to/blink)
  ✓ idf.toml: blink for esp32s3, ESP-IDF 6.0
  ✓ idf.lock: ESP-IDF 6.0.3 (76f5dedd99), Python 3.12.13
SDK
  ✓ ESP-IDF 6.0.3 at ~/.idf-next/sdk/6.0.3/esp-idf
  ✓ xtensa-esp-elf esp-15.2.0_20251204
  ...
Python
  ✓ Python 3.12.13 (~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv)
  ✓ ESP-IDF's check-python-dependencies passes

No problems found.
```

`doctor` is read-only and installs nothing. When it finds a problem, it prints a fix command under each item and exits with a non-zero status.

## Flash and monitor

Connect the board with USB:

```bash
idf run
```

`run` builds, flashes, and then opens the serial monitor, in that order. The serial port is found automatically by USB VID/PID. If only one board is connected, it is used directly. If several are connected, you are asked to choose one in the terminal. In the monitor, Ctrl+R resets the chip, and Ctrl+C or Ctrl+] exits.

You can also run the steps separately:

```bash
idf flash      # builds and flashes
idf monitor    # opens the serial monitor only
```

For details on ports, baud rates, and reset behavior, see [Build, flash, and monitor](/en/guide/build-flash-monitor).

::: warning ESP32-P4 earlier than v3
ESP-IDF builds for ESP32-P4 v3.x by default. If the chip revision is lower than v3, `idf flash` fails before it writes anything and suggests adding the following to `idf.toml`:

```toml
[config]
ESP32P4_SELECTS_REV_LESS_V3 = true
```
:::

## Common global options

Global options can be written before or after the command name. Exception: `size`, `efuse`, `secure`, `coredump`, `debug`, and `native` pass every argument that follows the command name to the upstream tool unchanged, so global options for these commands must be written before the command name.

| Option | Effect |
| --- | --- |
| `-C, --project <PATH>` | Runs in `PATH`, as if started there |
| `-t, --target <CHIP>` | Uses a different chip for this invocation only, without changing `idf.toml` |
| `-p, --port <PORT>` | Serial port. The environment variable `IDF_PORT` can also be used |
| `-r, --release` / `--profile <NAME>` | Selects the profile. The default is `dev` |
| `--offline` | `idf` itself does not access the network |
| `--locked` | Fails with an error if `idf.lock` would need to be changed |
| `--no-input` | Never prompts for input |
| `-v` / `-q` | More / less output |

## Next steps

- [Project and idf.toml](/en/guide/project): how to write each section of `idf.toml`, and what is in `build/`.
- [Configuration](/en/guide/config): change Kconfig options with `idf config`.
- [Dependencies](/en/guide/dependencies): add registry, git, or local components.
