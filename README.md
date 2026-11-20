# idf-next

A single Rust binary, `idf`, that drives ESP-IDF projects from one `idf.toml`.
The project directory holds only `idf.toml`, `idf.lock`, `src/` and an
optional `components/`; generated files go to `build/`, and SDKs, toolchains
and their Python venvs live in `~/.idf-next/`. Python is managed by `idf` and
never needs activating.

The full design and phase plan is in [docs/PLAN.md](docs/PLAN.md).

## Install

```sh
uv tool install idf-next
idf --version
```

The wheel's only Python dependency is `uv`, which `idf sdk install` uses to
create each SDK's venv.

Supported ESP-IDF releases: `>=6.0.0, <6.1.0`.

## Status

| Phase | Scope | State |
| --- | --- | --- |
| P0 | `idf.toml` / `idf.lock` schema, CLI skeleton, wheel packaging | done |
| P1 | `sdk`, `env`, `doctor` | done |
| P2 | `new`, `init`, `target`, `build`, `clean`, `native` | done |
| P2 | `run`, `flash`, `monitor`, `erase`, `image` | done |
| P3 | `config` | done |
| P4 | `add`, `remove`, `update`, `tree` | next |
| P5 | `size`, `debug`, `coredump`, `efuse`, `secure` | |

Commands from later phases already parse and say which phase delivers them.

On a terminal, `idf`, `idf --help` and `idf --version` draw the Espressif
symbol, rasterized for the terminal's cell shape (`NO_COLOR` is honoured;
piped output is plain). The symbol is a trademark of Espressif Systems.

## Projects

```sh
idf new blink -t esp32s3   # idf.toml, src/main.c, .gitignore
cd blink
idf build                  # build/esp32s3/dev/blink.bin
idf -r build               # release profile, in build/esp32s3/release/
idf -t esp32c6 build       # another chip, without touching idf.toml
idf target set esp32c6     # ...or change it in idf.toml
idf native menuconfig      # any idf.py action, on the same build directory
idf clean [--all]
```

`idf build` turns `idf.toml` into a CMake project under `build/` and runs
ESP-IDF's own CMake framework on it: every source file in `src/` (and
`src/Kconfig.projbuild`) forms the `main` component, `components/` holds your
own components, `[config]` becomes `sdkconfig.defaults` and `[[partition]]`
the partition table. Nothing but `idf.toml`, `idf.lock` and your sources live
in the project; `compile_commands.json` is a link into the latest build.

## Configuration

```sh
idf config get FREERTOS_HZ             # the value a build uses
idf config set FREERTOS_HZ=1000 LOG_DEFAULT_LEVEL=debug
idf -r config set COMPILER_OPTIMIZATION=size   # into [profile.release.config]
idf config diff                        # what differs from Kconfig's defaults, and why
idf config explain ESP_CONSOLE_UART_BAUDRATE   # help, dependencies, defaults
idf config menu                        # menuconfig; changes go back to idf.toml
```

Kconfig options live in `idf.toml` and nowhere else: `[config]` for every
build, `[profile.<name>.config]` on top for one profile. Write them by hand or
let `set` and `menu` do it (comments and formatting are kept). A choice is
set by its option, `LOG_DEFAULT_LEVEL_DEBUG = true`, which is what
`set LOG_DEFAULT_LEVEL=debug` writes. The next build starts `sdkconfig` over
from `idf.toml`, so a change there is all it takes; `idf build` warns about
entries Kconfig ignores (unknown names, values out of range, or options
whose prompt is hidden, e.g. `ESP_CONSOLE_UART_BAUDRATE` without
`ESP_CONSOLE_UART_CUSTOM`).

## Devices

```sh
idf run                    # build, flash, monitor
idf flash [--app] [--bootloader] [--partition-table]
idf monitor [--no-reset]   # Ctrl+R resets the chip, Ctrl+C (or Ctrl+]) exits
idf erase                  # whole flash
idf erase --partition nvs  # one partition, from the table on the board
idf erase --region 0x9000 24K
idf image [--format bin|uf2|dfu] [-o FILE]   # default: build/<target>/<profile>/
```

The port is found by USB VID/PID (Espressif USB, CP210x, CH34x, FTDI);
`-p <PORT>` or `IDF_PORT` picks one when several boards are attached.
Flashing runs at 460800 baud unless `-b` says otherwise, skips images the
board already holds, and refuses images built for another chip or chip
revision before writing anything. The monitor decodes code addresses and
RISC-V stack dumps against the firmware and ROM ELFs. `idf image` writes
exactly what `idf.py merge-bin`, `uf2` and `dfu` write.

ESP32-P4 boards before chip revision v3.0 need a different build; `idf
flash` says so, and the fix is one line in `idf.toml`:

```toml
[config]
ESP32P4_SELECTS_REV_LESS_V3 = true
```

## SDKs

```sh
idf sdk install          # in a project: install what idf.lock pins (writing it first if needed)
idf sdk install 6.0      # outside a project: newest 6.0.x
idf sdk use 6.0.3        # change idf.toml and re-pin
idf sdk list | path | remove <version>
idf env [--shell]        # environment for running upstream tools by hand
idf doctor               # check the tool, the project and its SDK
```

An SDK is the ESP-IDF source, its toolchains and a Python venv, all under
`~/.idf-next/sdk/<version>/` (override the root with `IDF_NEXT_HOME`).
Downloads are kept in `~/.idf-next/downloads/`, so `idf sdk remove` followed
by `idf --offline sdk install` rebuilds the same SDK without the network.
Set `IDF_GITHUB_ASSETS=dl.espressif.com/github_assets` to download through
Espressif's mirror, and `GITHUB_TOKEN` if the GitHub API rate limit bites.

## Development

```sh
cargo test
uvx maturin build --release   # wheel in target/wheels
```
