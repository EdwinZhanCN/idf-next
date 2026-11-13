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
| P1 | `sdk`, `env`, `doctor` | next |
| P2 | `build`, `run`, `flash`, `monitor`, `erase`, `image`, `native` | |
| P3 | `config` | |
| P4 | `add`, `remove`, `update`, `tree` | |
| P5 | `size`, `debug`, `coredump`, `efuse`, `secure` | |

Commands from later phases already parse and say which phase delivers them.

## Development

```sh
cargo test
uvx maturin build --release   # wheel in target/wheels
```
