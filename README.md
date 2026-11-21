# idf-next

A single Rust binary, `idf`, that drives ESP-IDF projects from one `idf.toml`.
SDKs, toolchains and Python are managed for you.

```sh
cargo install --git https://github.com/EdwinZhanCN/idf-next   # needs uv on PATH
idf new blink -t esp32s3 && cd blink
idf run          # build, flash, monitor
```

Supports ESP-IDF `>=6.0, <6.1`. Early work in progress.

Docs (English and 简体中文): https://edwinzhancn.github.io/idf-next/ · design notes: [docs/PLAN.md](docs/PLAN.md).

License: [MIT](LICENSE).
