---
title: Commands
description: All idf commands, arguments, and global options.
---

# Commands

This page lists all commands of `idf` 0.1.0. For option names and descriptions, `idf <command> --help` is authoritative. For behavior, the source code is authoritative.

```text
Usage: idf [OPTIONS] <COMMAND>
```

Commands are grouped into four sets:

| Group | Commands |
| --- | --- |
| Project | [`new`](#idf-new), [`init`](#idf-init), [`add`](#idf-add), [`remove`](#idf-remove), [`update`](#idf-update), [`tree`](#idf-tree), [`target`](#idf-target), [`config`](#idf-config) |
| Build and device | [`build`](#idf-build), [`run`](#idf-run), [`flash`](#idf-flash), [`monitor`](#idf-monitor), [`erase`](#idf-erase), [`image`](#idf-image), [`size`](#idf-size), [`clean`](#idf-clean) |
| Debugging and security | [`debug`](#idf-debug), [`coredump`](#idf-coredump), [`efuse`](#idf-efuse), [`secure`](#idf-secure) |
| Environment | [`sdk`](#idf-sdk), [`env`](#idf-env), [`doctor`](#idf-doctor), [`native`](#idf-native), [`completions`](#idf-completions) |

## Global options

The options below are available to all commands. They can be written before or after the subcommand (`idf -r build` and `idf build -r` are equivalent). Not every command uses every option. For example, `-p` is meaningful only for commands that connect to a serial port.

| Option | Description |
| --- | --- |
| `-C, --project <PATH>` | Run as if started in `PATH`. Searches upward from `PATH` for `idf.toml`. `PATH` must be an existing directory. `init` initializes directly in `PATH`. |
| `-t, --target <CHIP>` | Chip for this invocation only. Overrides `target` in `idf.toml`. Determines the build directory `build/<target>/<profile>/`. For `new` and `init`, it is the chip written to the new `idf.toml`. For the chip name validation rules, see [idf.toml](/en/reference/idf-toml#project). |
| `-p, --port <PORT>` | Serial port. Can be provided by the environment variable `IDF_PORT`. If neither is set, the `port` from the user config is used. If that is also unset, the port is detected automatically by USB VID/PID. See [serial port selection](#port). |
| `-r, --release` | Use the `release` profile. Mutually exclusive with `--profile`. |
| `--profile <NAME>` | Use the named profile. `dev` and `release` are built in. Other names must be declared in `idf.toml` as `[profile.<NAME>]`, or the command fails with `no profile named ...`. Defaults to `dev`. |
| `--offline` | Do not access the network. `idf`'s own downloads and GitHub queries fail immediately, and uv is invoked with its offline arguments. |
| `--locked` | Fail instead of modifying `idf.lock` when a change would be needed. See [idf.lock](/en/reference/idf-lock#locked). |
| `--no-input` | Do not prompt for input. Currently affects only serial port selection: if several boards are found, the command fails and lists the candidates. |
| `-v, --verbose...` | More output: shows the output of the CMake configuration step, the command lines executed, network retries, the full output of the component manager, and so on. Can be repeated, but the current implementation only distinguishes whether `-v` is given. Mutually exclusive with `-q`. |
| `-q, --quiet` | Less output: no status lines or progress bars, and no logo. Warnings and errors are still printed. |
| `-h, --help` | Print help. |
| `-V, --version` | Print the version. `-V` prints one line, `idf 0.1.0`. `--version` adds a line with the supported ESP-IDF range. |

```bash
$ idf --version
idf 0.1.0
supported ESP-IDF: >=6.0.0, <6.1.0
```

::: details --offline does not cover the component manager
`--offline` restricts the network access of `idf` itself (SDK sources, toolchains, the Python interpreter and packages, and the GitHub API). The ESP-IDF component manager, which is called during the CMake configuration step, does not receive this option. On the first configuration of a new build directory for a project with dependencies, the component manager still needs network access to read registry metadata. See [roadmap and status](/en/design/status).
:::

### Serial port selection {#port}

Commands that use a serial port (`run`, `flash`, `monitor`, `erase`, and `efuse` and `coredump` when they add a port automatically) pick the port in this order:

1. `-p/--port`;
2. the environment variable `IDF_PORT`;
3. the `port` in the user config `~/.config/idf/config.toml` (see [environment variables and user config](/en/reference/environment));
4. automatic detection: lists all USB serial ports and keeps only known USB VID/PIDs (Espressif `303a:*`, CP210x, CH340, CH343, CH9102, FTDI FT232/FT2232/FT232H/FT231X). On macOS, `/dev/tty.*` is ignored and only `/dev/cu.*` is used.
   - Exactly one: used directly.
   - None: the command fails with `no ESP board found on USB` and lists the other USB serial ports.
   - Several: the user is prompted to choose in the terminal. If `--no-input` is set or the command is not running in a terminal, the command fails with `several boards found: ...`.

### Commands that need a project and the SDK {#needs-project}

| Case | Commands |
| --- | --- |
| Does not need a project | `new`, `completions`, `sdk install <version>`, `sdk list`, `sdk remove`, `sdk path <version>` |
| Project optional | `init` (creates in the current directory or the `-C` directory), `target list`, `monitor`, `erase`, `doctor` |
| Needs a project, does not need the SDK | `target set`, `clean` |
| Needs a project, installs the SDK automatically when needed, writes `idf.lock` | `build`, `run`, `flash`, `image`, `size`, `config`, `add`, `remove`, `update`, `tree`, `debug`, `coredump`, `efuse`, `secure`, `native`, `env`, `sdk install` (without a version), `sdk use` |

If no project is found, the command fails:

```text
`idf build` needs a project, but no idf.toml was found here or in any parent directory
```

## Project

### idf new

Creates a project in a new directory.

```text
idf new [OPTIONS] <PATH>
```

| Argument | Description |
| --- | --- |
| `<PATH>` | New project directory. It must not exist. The directory name is the project name. |

- Generates `idf.toml`, `src/main.c`, and `.gitignore` (containing `/build/` and `/compile_commands.json`).
- The chip is taken from `-t`, and defaults to `esp32`. The `idf` field is written as `"6.0"`.
- The directory name must be a valid project name (starts with a letter, and contains only letters, digits, `-`, and `_`). Otherwise the command fails with `` `<name>` cannot be used as a project name ``.
- If the directory already exists, the command fails with `<PATH> already exists` and suggests using `idf init`.
- Does not access the network, does not install the SDK, and does not write `idf.lock`. `<PATH>` is resolved relative to the current working directory.

```bash
idf new blink -t esp32s3
```

Generated `idf.toml`:

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"
```

### idf init

Creates a project in the current directory (or the directory given by `-C`).

```text
idf init [OPTIONS]
```

- The generated content is the same as for `new`.
- If `idf.toml` already exists in the directory, the command fails with `<dir> already has an idf.toml`.
- If `src/` exists and is not empty, `src/main.c` is not written. If `.gitignore` exists, it is not overwritten.
- The project name is the directory name. The rules are the same as for `new`.

### idf add

Adds dependencies to `[dependencies]` in `idf.toml`, and writes the resolution result to `idf.lock`.

```text
idf add [OPTIONS] <SPECS>...
```

| Argument and option | Description |
| --- | --- |
| `<SPECS>...` | `namespace/name[@requirement]`, or a dependency name when used with `--git` or `--path`. |
| `--git <GIT>` | Get the component from a git repository (repository URL). Mutually exclusive with `--path`. |
| `--rev <REV>` | Branch, tag, or commit of the git repository. Requires `--git`. |
| `--path <PATH>` | Get the component from a local directory. The dependency is named after the directory. |

- For a registry dependency without a requirement, resolution first runs with `*`. Then `^<resolved version>` is written to `idf.toml`. If `@requirement` is given, it is written as is.
- `--git` and `--path` accept only one dependency name.
- A path dependency must be named after the component's directory, because the component manager looks up components by directory name. If the names do not match, the command fails and shows the correct form. If the path is inside the project, write it as a path relative to the project root.
- Resolution is done by the component manager (`idf_component_manager.prepare_components`). CMake is not run. Requires network access.
- After completion, `build.ninja` is deleted from the build directory. The next build reruns the CMake configuration, so CMake sees the new set of components. `remove` and `update` do the same.
- With `--locked`, the command fails immediately: `` `idf add` would change idf.toml and idf.lock, but --locked was given ``.
- Newly added entries have no `targets` or `idf` conditions. For conditional dependencies, edit `idf.toml` manually. See [idf.toml](/en/reference/idf-toml#dependencies).

```bash
idf add espressif/led_strip
idf add espressif/button@^4.1
idf add mylib --git https://example.com/mylib.git --rev v1.0
idf add mylib --path ../mylib
```

### idf remove

Removes dependencies and removes the entries in `idf.lock` that are no longer needed.

```text
idf remove [OPTIONS] <NAMES>...
```

- The names must be existing keys in `[dependencies]`. Otherwise the command fails and lists the existing dependencies.
- After removal, resolution runs again and `idf.lock` is updated. With `--locked`, the command fails.

### idf update

Updates `idf.lock` within the constraints in `idf.toml`.

```text
idf update [OPTIONS] [NAMES]...
```

| Argument | Description |
| --- | --- |
| `[NAMES]...` | Update only these dependencies (default: all). |

- With no names, all component locks in `idf.lock` are dropped. With names, those dependencies are dropped, along with indirect dependencies that were brought in only by them. Then resolution runs again.
- Prints version changes: `Updated <name> <old> -> <new>`, `Added <name> <version>`. If nothing changed, prints `Unchanged everything is up to date`.
- With `--locked`, the command fails.
- Updates only component dependencies, not the ESP-IDF version. To upgrade the SDK, use [`idf sdk use`](#idf-sdk-use).

### idf tree

Prints the component dependency tree.

```text
idf tree [OPTIONS]
```

- Reads the resolution result of the component manager from the last configuration first. If there is none, resolution runs first. Under `--offline`, the command fails when no result is available.
- The first line is `<project name> (ESP-IDF <version>)`. A component that appears more than once and has sub-dependencies is marked `(*)` and is not expanded again. A component that was not resolved is marked `(not resolved)`.
- If the project has no `[dependencies]`, the command prints only one line of information.

### idf target

Sets or lists the chip.

```text
idf target [OPTIONS] <COMMAND>
```

#### idf target set

```text
idf target set [OPTIONS] <CHIP>
```

Writes `target` to `idf.toml`, preserving the comments and formatting in the file. If the chip name is not in the known list, a warning is printed, and the SDK decides.

#### idf target list

```text
idf target list [OPTIONS]
```

Lists the known chips (non-preview chips of ESP-IDF v6.0). When run inside a project, the project's chip is marked `(this project)`.

```text
esp32
esp32s2
esp32s3
esp32c2
esp32c3
esp32c5
esp32c6
esp32c61
esp32h2
esp32p4
```

### idf config

Reads and modifies Kconfig configuration. All subcommands operate on the build directory of the current target and profile. Configuration is written only to `idf.toml`. No sdkconfig is produced in the project directory.

```text
idf config [OPTIONS] <COMMAND>
```

- If the build directory has not been configured yet, or the source files, components, or Kconfig files have changed, a CMake configuration runs first (about 3 seconds). If only `[config]` or the partition table has changed, CMake does not run, and the values are calculated directly with kconfgen (about 0.3 seconds).
- Values are calculated the same way as in a build: only the defaults generated from `idf.toml` are used, not the old sdkconfig.

For usage details, see [configuration](/en/guide/config).

#### idf config get

```text
idf config get [OPTIONS] <KEY>
```

Prints the effective value of a symbol, for use in scripts:

- `KEY` can be given with or without the `CONFIG_` prefix;
- a choice prints the name of the selected option; hex prints `0x…`; strings are printed without quotes; bool prints `true` / `false`;
- an inactive bool prints `false`; other inactive symbols fail with an error that states what they depend on and the current value of the dependency;
- if the name does not exist, the command fails and suggests similar names.

#### idf config set

```text
idf config set [OPTIONS] <ASSIGNMENTS>...
```

| Argument | Description |
| --- | --- |
| `<ASSIGNMENTS>...` | `KEY=VALUE` pairs. |

- Writes to `[config]`. If a profile is chosen explicitly with `-r` or `--profile`, writes to `[profile.<name>.config]`.
- Checked against the Kconfig type and range. bool accepts `true`, `y`, `yes`, `on`, `1`, and `false`, `n`, `no`, `off`, `0`. int and hex accept decimal, or hexadecimal starting with `0x`. Values outside `range` cause an error. Hex values are written as the string `"0x…"`.
- For a choice, write `CHOICE=OPTION`. `OPTION` can be the full option name, or a part of it with the choice name prefix removed. Matching is case-insensitive. For example, `LOG_DEFAULT_LEVEL=debug` is written as `LOG_DEFAULT_LEVEL_DEBUG = true`, and the other options of the same choice are removed. A choice option cannot be set to `false` on its own.
- Symbols without a prompt are rejected.
- Edits are made with toml_edit, which preserves comments. When an existing value is overwritten, its trailing comment is kept.
- After writing, the result is recalculated. An entry that is set but has no effect is reported with the reason (dependency not enabled, prompt hidden, selected by another option, or out of range).

```bash
idf config set FREERTOS_HZ=1000 LOG_DEFAULT_LEVEL=debug
idf -r config set COMPILER_OPTIMIZATION_PERF=y
```

#### idf config diff

```text
idf config diff [OPTIONS]
```

Lists the settable symbols that differ from the Kconfig defaults, in two groups: entries written in `idf.toml`, and entries that change as a consequence. A choice lists only the selected option. The output is TOML that can be pasted directly into `idf.toml`. Entries in `idf.toml` that equal the default are listed separately.

#### idf config explain

```text
idf config explain [OPTIONS] <KEY>
```

Explains one symbol: its type and range, the menu path, the current value and its source (`idf.toml [config]`, a profile, the Kconfig default, a select, or a value determined by other settings), the default, the dependencies and their current values, the prompt condition, the sources of select and imply, conditional defaults, the Kconfig file where it is defined, and the help text. For a choice, all options are listed, with the selected and default options marked.

#### idf config menu

```text
idf config menu [OPTIONS]
```

Runs the upstream menuconfig and writes the changes back to `idf.toml` when you exit.

- A terminal is required (both stdin and stdout must be terminals). Otherwise the command fails with `menuconfig needs a terminal`.
- menuconfig edits a sdkconfig generated from `idf.toml`, stored in `build/<target>/<profile>/menuconfig/`. The build's own sdkconfig is not affected.
- The write-back rules are the same as for `set`. Entries that were set back to the Kconfig default are removed from the table (after verifying that removing them does not change the result). After writing back, the result is recalculated, and a notice is shown if it does not match the menuconfig result.

## Build and device

### idf build

Builds the firmware.

```text
idf build [OPTIONS]
```

1. Ensures the SDK is installed (according to `idf.lock`; if there is no lock, resolves and writes it).
2. Generates the CMake project in `build/<target>/<profile>/project/`. Generated files are not rewritten if their content is unchanged.
3. If generated files have changed, or there is no `build.ninja` yet, runs the CMake configuration. Configuration output is shown only on failure or with `-v`.
4. If there are dependencies, merges the component manager's result into `idf.lock`.
5. If configuration was rerun, checks `[config]` for unknown keys, assignments to a choice, symbols without a prompt, and values that have no effect, and prints warnings.
6. Runs `cmake --build`. Ninja's output is passed through directly.
7. Creates a `compile_commands.json` symbolic link in the project root that points to this build. If a real file already exists there, it is left alone.

On completion, prints the path, size, and elapsed time of the application image. For the structure of the generated project, see [architecture](/en/design/architecture).

### idf run

Builds, flashes, and then opens the serial monitor.

```text
idf run [OPTIONS]
```

| Option | Description |
| --- | --- |
| `-b, --baud <BAUD>` | Flash baud rate. Defaults to the user config `flash_baud`, then to 460800. |

- The build is the same as `idf build`, and the flashing is the same as `idf flash`. However, the chip is not reset after flashing. The monitor resets it once after opening the port and setting the baud rate, so the first output is not lost.
- `-b` affects only flashing. The monitor's baud rate comes from the build's sdkconfig (see [`monitor`](#idf-monitor)).
- The chip revision is the value read when connecting for flashing. It is used to select the ROM ELF.

### idf flash

Builds and flashes.

```text
idf flash [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--app` | Flash only the application. |
| `--bootloader` | Flash only the bootloader. |
| `--partition-table` | Flash only the partition table. |
| `-b, --baud <BAUD>` | Flash baud rate. Defaults to the user config `flash_baud`, then to 460800. |

- Builds first, then writes all images listed in the build directory's `flasher_args.json` (including otadata and others). The three selection options can be combined. If none is given, all images are written. If the build does not contain a selected image, the command fails with `this build has no <role> image`.
- Images are written as is. Like esptool, they are padded to a 4-byte boundary with `0xFF`. Images that are already identical on the board are skipped by MD5 comparison.
- Checks before writing: the chip ID and chip revision range in the image header, and whether the image exceeds the board's flash. If a check fails, the chip is reset to its previous firmware. If the flash size configured in the build is larger than the actual size of the board, only a warning is shown.
- Resets once after writing.

::: tip ESP32-P4 before v3
If the chip revision does not meet the image requirement, the command fails. Boards with ESP32-P4 before v3 must set `ESP32P4_SELECTS_REV_LESS_V3 = true` in `[config]`, and the error message suggests this.
:::

### idf monitor

Serial monitor, with address and stack trace decoding.

```text
idf monitor [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--no-reset` | Do not reset the chip. Attach to the running firmware directly. |
| `-b, --baud <BAUD>` | Baud rate. |

- Does not build and does not install the SDK. It can be used outside a project, but without decoding.
- Baud rate: `-b`. Otherwise `ESP_CONSOLE_UART_BAUDRATE` from the sdkconfig of the current target and profile build. Otherwise `ESPTOOLPY_MONITOR_BAUD`. Otherwise 115200. The user config `flash_baud` does not affect the monitor.
- ELFs used for decoding: the application ELF (from `project_description.json` in the build directory), and the ROM ELFs from `esp-rom-elfs` in the SDK. ROM ELFs are available only when the SDK locked in `idf.lock` is installed. When run on its own, the chip revision is taken from `ESP_REV_MIN_FULL` in the sdkconfig. If the build has no ELF, a warning is shown.
- By default, the chip is reset after the port is opened.
- Keys: Ctrl+R resets. Ctrl+C or Ctrl+] exits. Other keys are sent to the board.
- When the port disappears, prints `<port> disconnected; waiting for it`. When a port with the same name comes back, prints `<port> is back` and continues.
- If stdout is not a terminal or `NO_COLOR` is set, color sequences are removed.

### idf erase

Erases the entire flash, one partition, or a region.

```text
idf erase [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--partition <NAME>` | Erase one partition by name. The partition table is read from the board. Mutually exclusive with `--region`. |
| `--region <OFFSET> <SIZE>` | Erase `SIZE` bytes at `OFFSET`. Must be a non-zero multiple of 4K, for example `0x9000 24K`. |
| `-b, --baud <BAUD>` | Baud rate. Defaults to the user config `flash_baud`, then to 460800. |

- With no option, the entire flash is erased.
- Does not build and does not need the SDK. Inside a project, the connected chip is checked against the project's chip.
- `OFFSET` and `SIZE` use the same syntax as partition sizes in `idf.toml`: integers, `0x…`, `24K`, `1M`. Alignment is checked before the chip is connected.
- `--partition` reads the partition table from the board. The offset is `PARTITION_TABLE_OFFSET` from the build sdkconfig, defaulting to `0x8000`. If the board has no valid partition table, or the partition does not exist, the command fails and lists the partitions on the board.
- Resets into the application after erasing.

::: warning
Erasing is not reversible. `idf erase` with no option erases the entire flash, including data in NVS.
:::

### idf image

Exports a merged firmware image.

```text
idf image [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--format <FORMAT>` | `bin` (default), `uf2`, or `dfu`. |
| `-o, --output <OUTPUT>` | Output file. |

- Builds first, then merges the images according to `flasher_args.json`.
- `bin`: equivalent to `esptool merge-bin`. Each image is placed at its own offset, gaps are filled with `0xFF`, and the image is meant to be flashed from address 0x0. The default output is `build/<target>/<profile>/merged-binary.bin`.
- `uf2`: equivalent to `esptool merge-bin --format uf2`. The default output is `uf2.bin`.
- `dfu`: equivalent to `tools/mkdfu.py`. Supports only esp32s2, esp32s3, and esp32p4. The block size can be set with the environment variable `ESP_DFU_PART_SIZE`. The default output is `dfu.bin`.
- The output of all three formats is byte-identical to `idf native merge-bin / uf2 / dfu` (verified by differential testing on an ESP32-P4 build).

### idf size

Firmware size analysis. Calls `esp-idf-size`.

```text
idf size [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `esp-idf-size`. |

- Runs `python -m esp_idf_size <ARGS>`. If none of the arguments ends with `.map`, `<project name>.map` from the build directory is appended at the end.
- Does not build first. If no map file exists, the command fails and suggests running `idf build`.

```bash
idf size --format json2
```

### idf clean

Cleans build artifacts.

```text
idf clean [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--all` | Delete all targets and profiles, that is, the entire `build/` directory. |

- Without `--all`, deletes the directory of the current target and profile, `build/<target>/<profile>/`. This is affected by `-t`, `-r`, and `--profile`.
- If the `compile_commands.json` symbolic link in the project root points to a build that is deleted, the link is deleted too.

`clean` does not touch the global cache `~/.idf-next/`. To delete an SDK, use [`idf sdk remove`](#idf-sdk-remove).

## Debugging and security

These four commands are thin wrappers around upstream tools. `idf` adds the arguments it already knows (chip, serial port, ELF path), runs the upstream tool in the SDK's Python environment, and returns its exit code unchanged.

::: tip Argument passthrough
After the command name, every argument from the first one that `idf` does not recognize is passed unchanged to the upstream tool. Options of `idf` itself that immediately follow the command name (such as `--help` or `-p`) are still parsed by `idf`. To pass them to the upstream tool, write `--` first, for example `idf size -- --help`.
:::

### idf debug

Starts OpenOCD and GDB.

```text
idf debug [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `idf.py gdb`. |

- After generating the CMake project, runs `idf.py -C build/<t>/<p>/project -B build/<t>/<p> openocd gdb <ARGS>`.
- If the environment variable `OPENOCD_COMMANDS` is not set, the selected serial port is the chip's built-in USB-Serial-JTAG (`303a:1001`), and the SDK's OpenOCD scripts contain `board/<chip>-builtin.cfg`, then `OPENOCD_COMMANDS=-f board/<chip>-builtin.cfg` is set automatically. Port detection here does not prompt for a selection.
- If `ESP32P4_SELECTS_REV_LESS_V3=y` is set in the build's sdkconfig, `-c 'gdb_memory_map disable'` is appended. On ESP32-P4 before v3, OpenOCD cannot read the flash memory map, so breakpoints in flash use hardware breakpoints.
- Other debug adapters (FTDI or UART bridge boards) use the SDK's default OpenOCD arguments. You must set `OPENOCD_COMMANDS` yourself.

### idf coredump

Parses core dumps. Calls `esp-coredump`.

```text
idf coredump [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `esp-coredump`. |

Rules for adding arguments (with `--help` / `-h`, only the chip is added):

- If `--chip` is not present, `--chip <target>` is added;
- If `ARGS` is not empty, and none of `--core`, `-c`, `--port`, or `-p` is present, the selected serial port is added (the dump is read from flash);
- If `ARGS` is not empty, and no argument ends with `.elf`, the application ELF is appended at the end. If the build has no ELF, the command fails.

If `ARGS` is empty, only `--chip` is added.

::: warning
`coredump` has been verified only for command assembly. It has not yet been tested with a real core dump.
:::

### idf efuse

Reads, summarizes, and burns eFuses. Calls `espefuse`.

```text
idf efuse [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `espefuse`. |

- If `--chip` / `-c` is not present, `--chip <target>` is added.
- If `--virt`, `--port` / `-p`, or `--help` / `-h` is not present, `--port <selected serial port>` is added.
- On a real chip (without `--virt`), two additional protections apply, both before the chip is connected:
  - `--do-not-confirm` is refused;
  - burn subcommands require the user to type `burn` manually in the terminal. If stdin or stderr is not a terminal, the command is refused.
- In virtual mode (`--virt`), these two protections do not apply, and no port is added.

Burn subcommands (require typing `burn`): `burn-efuse`, `burn-block-data`, `burn-bit`, `burn-key`, `burn-key-digest`, `burn-custom-mac`, `set-flash-voltage`, `write-protect-efuse`, `read-protect-efuse`, `execute-scripts`.

Read-only subcommands: `summary`, `dump`, `get-custom-mac`, `adc-info`, `check-error`.

```bash
idf efuse summary
idf efuse --virt burn-efuse ...
```

::: warning
eFuse burns are irreversible. A wrong value can make the chip unusable. In addition to the two protections above, espefuse's own confirmation still applies. The two lists are checked against the `--help` output of the installed `espefuse` by `tests/efuse_commands.rs`.
:::

### idf secure

Keys, signing, and flash encryption. Calls `espsecure`.

```text
idf secure [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `espsecure`. |

No arguments are added, and the chip is not connected. The command simply runs `python -m espsecure <ARGS>` in the SDK's Python environment.

## Environment

### idf sdk

Installs and selects the ESP-IDF SDK. An SDK consists of the source code, the toolchain, and a Python venv. It is installed in `~/.idf-next/sdk/<version>/`.

```text
idf sdk [OPTIONS] <COMMAND>
```

#### idf sdk install

```text
idf sdk install [OPTIONS] [VERSION]
```

| Argument | Description |
| --- | --- |
| `[VERSION]` | For example `6.0` or `6.0.1`. Default: the version the project requires. |

- With a version: resolves the latest matching stable release (through the GitHub API; under `--offline`, it is looked up among the installed SDKs), and installs it. Does not write `idf.lock`.
- Without a version: requires a project. Installs exactly the version in `idf.lock`. If there is no lock, or the lock does not match `idf.toml`, resolves the latest version, installs it, and writes the lock.
- If the version is outside the supported range, the command fails. For example: `ESP-IDF 5.4 is not supported by idf-next 0.1.0; supported: >=6.0.0, <6.1.0`.
- Parts that are already installed are not installed again. While installing, the file lock `~/.idf-next/.lock` is held, and another `idf` process waits for it.

```bash
idf sdk install 6.0
idf --offline --locked sdk install
```

#### idf sdk list

```text
idf sdk list [OPTIONS]
```

Lists the installed SDKs: version, path, and a note in parentheses (Python version, `incomplete`, `not supported by this idf-next`, or `used by this project`).

#### idf sdk use

```text
idf sdk use [OPTIONS] <VERSION>
```

Pins the project to an SDK version. Changes `idf` in `idf.toml` (preserving the formatting and comments), then installs and relocks as needed. `VERSION` can be `6.0` or `6.0.3`. With `--locked`, the command fails if `idf.toml` would need to change.

#### idf sdk remove

```text
idf sdk remove [OPTIONS] <VERSION>
```

Deletes `~/.idf-next/sdk/<VERSION>/`. `VERSION` must be an exact version (such as `6.0.3`). The download cache is kept, so the SDK can be reinstalled later with `--offline`. If the current project's `idf.lock` uses this version, a warning is shown.

#### idf sdk path

```text
idf sdk path [OPTIONS] [VERSION]
```

Prints the SDK's `IDF_PATH` (`~/.idf-next/sdk/<version>/esp-idf`). With no version, uses the version in the project's `idf.lock`. If a version is given, it must be an exact version. If the SDK is not installed, the command fails.

### idf env

Shows the SDK environment.

```text
idf env [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--shell` | Outputs `export` statements for a POSIX shell. |

Requires a project. If the SDK is not installed, it is installed first. If there is no lock, one is written. For the variables that are output, see [environment variables and user config](/en/reference/environment#idf-env).

```bash
eval "$(idf env --shell)"
```

### idf doctor

Diagnoses the environment. Read-only. Does not install anything.

```text
idf doctor [OPTIONS]
```

Checks:

- **Tool**: the tool version and the supported IDF range, the host platform, the uv version and path, and the SDK cache location.
- When inside a project:
  - **Project**: whether `idf.toml` can be parsed; whether `idf.lock` exists and matches `idf.toml`;
  - **SDK**: whether the SDK locked in the lock is installed and matches the lock; each tool is run with `version_cmd` from tools.json, and its version is compared using `version_regex`;
  - **Python**: whether the venv's interpreter can run; runs ESP-IDF's own `idf_tools.py check-python-dependencies`.
- When outside a project: **Installed SDKs**, which lists the installed SDKs and marks incomplete versions or versions outside the supported range.

If any check fails, the command exits with a non-zero status and prints `<n> problem(s) found`.

### idf native

Runs the upstream `idf.py` in the managed environment.

```text
idf native [OPTIONS] [ARGS]...
```

| Argument | Description |
| --- | --- |
| `[ARGS]...` | Arguments passed to `idf.py`. |

- Generates the CMake project first, then runs `idf.py -C build/<t>/<p>/project -B build/<t>/<p> <ARGS>`. It shares the same build directory and environment variables as `idf build`.
- The exit code is passed through unchanged.
- Used for upstream features that `idf` does not yet cover, such as `idf native merge-bin`.

::: warning
If `idf native … monitor` is killed, the child processes of `idf.py` do not exit with it. Pressing Ctrl+C during interactive use is not affected, because the signal is sent to the whole process group.
:::

### idf completions

Generates a shell completion script.

```text
idf completions [OPTIONS] <SHELL>
```

| Argument | Description |
| --- | --- |
| `<SHELL>` | `bash`, `elvish`, `fish`, `powershell`, or `zsh`. |

```bash
idf completions zsh > ~/.zfunc/_idf
```
