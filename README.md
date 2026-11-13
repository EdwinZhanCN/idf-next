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
| P2 | `run`, `flash`, `monitor`, `erase`, `image` | next |
| P3 | `config` | |
| P4 | `add`, `remove`, `update`, `tree` | |
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
