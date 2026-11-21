---
title: Environment variables and user config
description: The environment variables idf reads, the keys of ~/.config/idf/config.toml, their precedence, and the variables printed by idf env.
---

# Environment variables and user config

Settings that depend on the machine (serial port, flash baud rate, cache location) are not written in `idf.toml`. They come from the command line, environment variables, or the user config file. This page lists all environment variables that `idf` reads (taken from the `env::var` calls in `src/`), the keys of the user config file, and the environment that `idf` passes to upstream tools.

## Precedence

When one setting has more than one source, the precedence from highest to lowest is:

1. Command-line option;
2. Environment variable;
3. User config file `~/.config/idf/config.toml`;
4. Built-in default.

| Setting | Command line | Environment variable | User config | Built-in default |
| --- | --- | --- | --- | --- |
| Serial port | `-p, --port` | `IDF_PORT` | `port` | Auto-detected from USB VID/PID |
| Flash baud rate (`flash`, `run`, `erase`) | `-b, --baud` | None | `flash_baud` | 460800 |
| Monitor baud rate (`monitor`) | `-b, --baud` | None | None | `ESP_CONSOLE_UART_BAUDRATE` from sdkconfig, then `ESPTOOLPY_MONITOR_BAUD`, finally 115200 |
| Global cache location | None | `IDF_NEXT_HOME` | None | `~/.idf-next` |
| User config file location | None | `IDF_CONFIG`, then `XDG_CONFIG_HOME` | None | `~/.config/idf/config.toml` |

`idf run -b` only affects flashing. The monitor started by `run` always uses the baud rate from sdkconfig.

## User config file {#user-config}

```toml
# ~/.config/idf/config.toml
port = "/dev/cu.usbmodem1101"
flash_baud = 921600
```

| Key | Type | Description |
| --- | --- | --- |
| `port` | string | Serial port. Used when neither `-p` nor `IDF_PORT` is given. Applies to all commands that use a serial port. |
| `flash_baud` | integer | Flash baud rate. Used by `flash`, `run` and `erase` when `--baud` is not given. Does not affect `monitor`. |

- The file is optional. When it does not exist, the config is empty.
- Only these two keys are allowed. An unknown key is an error. The error message starts with `parsing <file path>`, so that a misspelled key is not silently ignored.
- Every command reads this file at startup, except `--help` and `--version`. An error in the file therefore makes every command fail.

The file location is the first of the following that is available:

1. `$IDF_CONFIG` (the full path of the file);
2. `$XDG_CONFIG_HOME/idf/config.toml`;
3. `~/.config/idf/config.toml`.

An empty string is treated as unset.

## Environment variables read by idf

### Device and output

| Variable | Purpose |
| --- | --- |
| `IDF_PORT` | Serial port, same as `-p`. `-p` takes precedence. |
| `NO_COLOR` | If set (to any value), no color is output: status lines and warnings are not colored, `monitor` strips color sequences from firmware output, and the logo is not colored. |
| `OPENOCD_COMMANDS` | Arguments passed to OpenOCD by `idf debug`. If set, used as is. If not set, `idf debug` sets it automatically when it detects the chip's built-in USB-Serial-JTAG. See [Commands](/en/reference/commands#idf-debug). |
| `ESP_DFU_PART_SIZE` | Chunk size for `idf image --format dfu`, in decimal or in hexadecimal with a `0x` prefix. If not set, 512 KB, the same default as `mkdfu.py`. |

### Cache and tool location

| Variable | Purpose |
| --- | --- |
| `IDF_NEXT_HOME` | Root directory of the global cache. Default `~/.idf-next`. See [Global cache](#home). |
| `IDF_CONFIG` | Path of the user config file. |
| `XDG_CONFIG_HOME` | Base directory for the user config file. The file used is `$XDG_CONFIG_HOME/idf/config.toml`. |
| `IDF_NEXT_UV` | The uv executable used to create the SDK venv. If set, it is used first. Otherwise, the `uv` in the same directory as the real path of `idf` is used (this is how `uv tool install idf-next` installs it). Otherwise, the `uv` on `PATH` is used. |

### Network

| Variable | Purpose |
| --- | --- |
| `GITHUB_TOKEN` | Used to authenticate GitHub API requests when querying ESP-IDF release versions, to avoid the rate limit for anonymous requests. On a 403 or 429 error, the error message suggests setting it. |
| `IDF_GITHUB_ASSETS` | Mirror for toolchain downloads. Same meaning as in `idf_tools.py`: `https://github.com/...` is rewritten to `https://$IDF_GITHUB_ASSETS/...`. |

### Terminal logo

The following variables only affect the logo that `idf`, `idf --help` and `idf --version` draw in the terminal. They do not affect other output:

| Variable | Purpose |
| --- | --- |
| `TERM` | When `dumb`, the logo is not drawn. When `linux`, `ansi` or `vt*`, only red is used. |
| `COLORTERM` | When `truecolor` or `24bit`, 24-bit color is used. Otherwise 256 colors are used. |
| `LC_ALL`, `LC_CTYPE`, `LANG` | If the locale is not UTF-8, ASCII characters are used instead. |

The logo is also not drawn when stdout is not a terminal, or when `-q` is used. The output received by pipes and scripts is identical to the output without a logo.

## Global cache {#home}

Layout under `IDF_NEXT_HOME` (default `~/.idf-next`):

```text
~/.idf-next/
  sdk/<version>/                  one SDK, installed and removed as a unit
    esp-idf/                      source (no .git; version.txt added)
    tools/<tool>/<version>/       toolchains from SDK tools.json
    python/<first 16 of digest>/   one venv per Python resolution
      venv/
      espidf.constraints.vX.Y.txt
      requirements.txt
      env.toml                    written last: marks venv complete
    sdk.toml                      written last: marks source and tools complete
  downloads/                      hash-addressed download store (for offline reinstall)
  python/                         uv-managed Python interpreter
  components/                     component manager download cache, shared by all projects
  uv-cache/                       uv wheel cache (hard-linked into venvs)
  .lock                           file lock held during SDK install/remove
```

`idf sdk remove` only deletes `sdk/<version>/`. `downloads/`, `python/` and `uv-cache/` are kept, so `--offline` reinstallation is possible.

## Environment passed by idf to upstream tools {#upstream}

When `idf` runs CMake, Ninja, `idf.py` and other upstream Python tools, it sets the following variables on top of the current environment. No environment needs to be activated, and `export.sh` is not needed.

### SDK environment

Printed by `idf env` (see the next section). All commands that call upstream tools set these:

| Variable | Value |
| --- | --- |
| `IDF_PATH` | `~/.idf-next/sdk/<version>/esp-idf` |
| `IDF_TOOLS_PATH` | Python environment directory `~/.idf-next/sdk/<version>/python/<first 16 digits of digest>`, which contains `espidf.constraints.vX.Y.txt` |
| `IDF_PYTHON_ENV_PATH` | `venv` within that directory |
| `ESP_IDF_VERSION` | Release series, such as `6.0` |
| Variables exported by tools | From `export_vars` in `tools.json`, for example `OPENOCD_SCRIPTS` and `ESP_ROM_ELF_DIR` |
| `PATH` | The venv `bin`, each tool's `export_paths` and `esp-idf/tools` are prepended, in that order |

`IDF_TOOLS_PATH` and `IDF_PYTHON_ENV_PATH` point to the managed venv and constraints file. As a result, IDF's own `idf_tools.py check-python-dependencies` (which CMake runs before every build) passes unchanged.

### Build directory related

Build, configure and thin-wrapper commands also set:

| Variable | Value | Reason |
| --- | --- | --- |
| `IDF_TARGET` | Current chip | Selects the target |
| `IDF_SKIP_CHECK_SUBMODULES` | `1` | The SDK has no `.git`, so there are no submodules to check |
| `IDF_COMPONENT_MANAGER` | `1` when there are dependencies, otherwise `0` | Prevents the component manager from slowing down configuration when there are no dependencies |
| `IDF_COMPONENT_CACHE_PATH` | `~/.idf-next/components` | The component download cache is shared between projects |

CMake configuration also passes `-DPYTHON=<python in the venv>`, `-DIDF_TARGET=<chip>` and `-DCCACHE_ENABLE=0`.

### uv

When uv is invoked, `idf` sets `UV_CACHE_DIR=~/.idf-next/uv-cache`, `UV_PYTHON_INSTALL_DIR=~/.idf-next/python` and `UV_NO_CONFIG=1`, and removes `VIRTUAL_ENV`, `UV_PYTHON`, `UV_INDEX_URL` and `UV_EXTRA_INDEX_URL`. In other words, the user's own uv configuration and any activated venv do not affect the creation of the SDK venv.

## Variables printed by idf env {#idf-env}

`idf env` prints the SDK environment above. It must be run in a project. If the SDK is not installed, it is installed first. If there is no lock, one is written.

```bash
$ idf env
IDF_PATH             ~/.idf-next/sdk/6.0.3/esp-idf
IDF_TOOLS_PATH       ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c
IDF_PYTHON_ENV_PATH  ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv
ESP_IDF_VERSION      6.0
OPENOCD_SCRIPTS      ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/v0.12.0-esp32-20260703/openocd-esp32/share/openocd/scripts
ESP_ROM_ELF_DIR      ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528/
PATH                 ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf-gdb/17.1_20260402/xtensa-esp-elf-gdb/bin
                     ~/.idf-next/sdk/6.0.3/tools/riscv32-esp-elf-gdb/17.1_20260402/riscv32-esp-elf-gdb/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf/esp-15.2.0_20251204/xtensa-esp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/riscv32-esp-elf/esp-15.2.0_20251204/riscv32-esp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/esp32ulp-elf/2.38_20240113/esp32ulp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/cmake/4.0.3/CMake.app/Contents/bin
                     ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/v0.12.0-esp32-20260703/openocd-esp32/bin
                     ~/.idf-next/sdk/6.0.3/tools/ninja/1.12.1
                     ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528
                     ~/.idf-next/sdk/6.0.3/esp-idf/tools
```

(macOS arm64, ESP-IDF 6.0.3. The actual output uses absolute paths. The home directory is written as `~` here.)

`--shell` prints `export` statements for POSIX shells. Values are enclosed in single quotes, and `PATH` is prepended to the existing `PATH`. The output below is abridged:

```bash
$ idf env --shell
export IDF_PATH='/Users/me/.idf-next/sdk/6.0.3/esp-idf'
export IDF_TOOLS_PATH='/Users/me/.idf-next/sdk/6.0.3/python/14645654ec2ff38c'
...
export PATH='/Users/me/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin:...:/Users/me/.idf-next/sdk/6.0.3/esp-idf/tools:'"${PATH}"
```

To use these tools in the current shell:

```bash
eval "$(idf env --shell)"
```

::: tip
Daily use does not need `idf env`. Every `idf` command sets up its environment by itself. `idf env` is for calling the toolchain directly outside `idf`, for example running `xtensa-esp-elf-gdb` from an editor or a script.
:::
