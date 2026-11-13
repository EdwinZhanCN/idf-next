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
| P2 | `build`, `run`, `flash`, `monitor`, `erase`, `image`, `native` | next |
| P3 | `config` | |
| P4 | `add`, `remove`, `update`, `tree` | |
| P5 | `size`, `debug`, `coredump`, `efuse`, `secure` | |

Commands from later phases already parse and say which phase delivers them.

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
