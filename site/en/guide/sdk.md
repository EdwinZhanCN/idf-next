---
title: SDK and environment
description: The five subcommands of idf sdk, env and doctor, the layout of the global cache at ~/.idf-next/, and how to reproduce the same environment offline.
---

# SDK and environment

An SDK consists of three parts: the ESP-IDF source, the toolchains listed in that version's `tools/tools.json`, and the Python venv for that version. All three are installed together under `~/.idf-next/sdk/<version>/` and are deleted together. A project refers to an exact SDK version only through `idf.lock`.

You do not need to run `export.sh`, and you do not need to activate the venv. Every `idf` command sets up the SDK environment itself.

## idf sdk

### install

```bash
idf sdk install          # inside a project: install the SDK pinned by idf.lock
idf sdk install 6.0      # install the latest 6.0 series release, without changing any project
idf sdk install 6.0.3    # install an exact version
```

```text
       Ready ESP-IDF 6.0.3 with Python 3.12.13 at ~/.idf-next/sdk/6.0.3
```

Inside a project with no version given, if `idf.lock` and `idf.toml` agree, the SDK is installed from the lock. Otherwise, the latest version that `idf.toml` requires is resolved on GitHub, installed, and written to `idf.lock`. With `--locked`, an error is reported if the lock is missing or would need to change. With `--offline`, GitHub is not queried, and only the installed, complete SDKs are searched for the latest matching version.

You rarely need to install manually. Commands that need the SDK, such as `build`, `env`, and `config`, perform the same steps automatically when the SDK is missing.

### use

```bash
idf sdk use 6.0.3
```

Changes `[project] idf` in `idf.toml` to the given version, keeping the file's format and comments. Then it locks and installs. With `--locked`, changing `idf.toml` is refused.

### list

```bash
idf sdk list
```

```text
6.0.3      ~/.idf-next/sdk/6.0.3  (Python 3.12.13, used by this project)
```

Parentheses indicate conditions such as an incomplete installation, a version outside the range this tool supports, or a version used by the current project.

### remove

```bash
idf sdk remove 6.0.3
```

Accepts only an exact version. `6.0` is an error: `give an exact version such as 6.0.0`. It removes only `sdk/<version>/`. The download cache, the Python interpreters, and the uv cache are kept, so the SDK can be reinstalled offline later. If the `idf.lock` of the current project uses this version, a warning is shown.

### path

```bash
idf sdk path           # the version pinned by the project's idf.lock
idf sdk path 6.0.3
```

Prints the `IDF_PATH` of the SDK, for example `~/.idf-next/sdk/6.0.3/esp-idf`. Like `remove`, it accepts only an exact version.

## idf env

Prints the environment that `idf` uses when it runs upstream tools. If necessary, the SDK is installed first.

```bash
idf env
```

```text
IDF_PATH             ~/.idf-next/sdk/6.0.3/esp-idf
IDF_TOOLS_PATH       ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c
IDF_PYTHON_ENV_PATH  ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv
ESP_IDF_VERSION      6.0
OPENOCD_SCRIPTS      ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/…/share/openocd/scripts
ESP_ROM_ELF_DIR      ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528/
PATH                 ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf/esp-15.2.0_20251204/xtensa-esp-elf/bin
                     …
                     ~/.idf-next/sdk/6.0.3/esp-idf/tools
```

`--shell` prints POSIX shell `export` statements, so the SDK tools can be used directly in the current shell:

```bash
eval "$(idf env --shell)"
```

`IDF_TOOLS_PATH` points to the directory that contains the venv, where Espressif's constraints file is stored. `IDF_PYTHON_ENV_PATH` points to the venv. As a result, IDF's own `idf_tools.py check-python-dependencies` (which CMake runs before every build) passes unchanged.

## idf doctor

A read-only check. It does not install or modify anything:

- Tools: the `idf` version and its supported IDF range, the host platform, uv, and the cache directory.
- In a project: whether `idf.toml` and `idf.lock` agree; whether the SDK pinned by the lock is installed and matches the lock; for each tool, running the `version_cmd` from tools.json and comparing the version exactly against `version_regex`; whether Python can run; and whether IDF's `check-python-dependencies` passes.
- Outside a project: lists the installed SDKs and marks those that are incomplete or outside the supported range.

Each problem is followed by a fix command. If any check fails, `doctor` exits with a non-zero status. If there are only warnings, it exits normally. For an example of the output, see [Quick start](/en/guide/getting-started#check-env).

## Global cache layout

```text
~/.idf-next/
  sdk/<version>/                  one SDK, installed and removed as a whole
    esp-idf/                      source (no .git; a version.txt is written instead)
    tools/<tool>/<version>/       toolchains listed in tools.json
    python/<digest>/              one venv per resolution result
      venv/
      espidf.constraints.vX.Y.txt
      requirements.txt
      env.toml                    written last, marks the venv as complete
    sdk.toml                      written last, marks the source and toolchains as complete
  downloads/                      download store addressed by hash (used for offline reinstall)
  python/                         Python interpreters managed by uv
  components/                     download cache of the component manager, shared by all projects
  uv-cache/                       uv's wheel cache (hard-linked into each venv)
```

- The root directory can be moved elsewhere with the environment variable `IDF_NEXT_HOME`.
- All writes hold the file lock `~/.idf-next/.lock`.
- `sdk.toml` and `env.toml` are written last. An installation interrupted partway shows as incomplete in `sdk list`. Run `idf sdk install <version>` again to finish it.

## What installation does

**IDF source**: the official release zip is about 2 GB, of which 1.86 GB is `.git`. `idf` reads the zip's central directory first, then downloads only the entries that are not in `.git`, using HTTP Range requests (about 130 MB). Each entry is checked against its CRC-32. A digest of the whole source tree is then computed and written to `[idf] source` in `idf.lock`.

**Toolchains**: reads `tools/tools.json` from the SDK, applies `platform_overrides` for the current host, and installs every tool marked `install: always`. It also installs `cmake` and `ninja`, so the system CMake and Ninja are not needed. Archives are verified by sha256 and size. Downloads and extraction run in parallel with 4 threads.

**Python environment**: runs `uv pip compile --universal --generate-hashes --exclude-newer <time>` with uv. The inputs are the non-optional requirements of the SDK and Espressif's constraints file. The default interpreter is Python 3.12. Only PyPI is used, not Espressif's extra pip index. The resolution result and the constraints file are stored in the download store, addressed by digest. The venv directory is named by the digest of the resolution result, so projects whose resolution results differ for the same SDK version do not overwrite each other.

**Mirrors**: the environment variable `IDF_GITHUB_ASSETS` is supported, with the same semantics as in `idf_tools.py`: `https://github.com/…` is rewritten to `https://$IDF_GITHUB_ASSETS/…`. Resolving an IDF version calls the GitHub API. Setting `GITHUB_TOKEN` avoids the rate limit for anonymous access.

## Offline reproduction {#offline}

`idf.lock` together with the download cache is enough to rebuild the same SDK without network access:

```bash
idf sdk remove 6.0.3
idf --offline --locked sdk install
```

- `--offline`: `idf` itself does not access the network. If a download is needed and the cache does not have it, an error is reported (for example, `… needs the network, but --offline was given`).
- `--locked`: if the lock is missing or would need to change, an error is reported instead of rewriting it.

This was verified on macOS arm64 with ESP-IDF v6.0.3. After deletion, the rebuilt SDK (source, toolchains, and venv, 50,559 files in total) was byte-identical to the one before deletion, and `idf.lock` did not change. When Python dependencies are missing, `idf` re-resolves them from the same inputs recorded in the lock. The digest of the result must match the lock.

::: warning Known limitations
- Third-party components are not covered by this guarantee. When a new build directory is configured for the first time, the component manager still needs network access to read registry metadata, and `--offline` does not prevent that. See [Dependencies](/en/guide/dependencies).
- Running upstream Python scripts writes `__pycache__` into the shared SDK directory. This does not affect correctness, but the SDK directory is no longer read-only after use.
:::
