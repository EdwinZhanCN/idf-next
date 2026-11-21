---
title: Configuration
description: "Read and write Kconfig options with idf config: how values are written in idf.toml, the choice syntax, profiles, and build-time checks."
---

# Configuration

Kconfig options are stored only in `idf.toml`. `[config]` applies to all profiles, and `[profile.<name>.config]` is layered on top of it. Each build generates `sdkconfig.defaults` from them. When they change, the `sdkconfig` in the build directory is deleted and regenerated, so hand edits to `build/` do not persist.

## Syntax in idf.toml

```toml
[config]
FREERTOS_HZ = 1000                 # int
ESP_TASK_WDT_EN = false            # bool
PARTITION_TABLE_OFFSET = "0x9000"  # hex: write as a "0x…" string
LOG_DEFAULT_LEVEL_DEBUG = true     # choice: write the selected option

[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true
```

| Kconfig type | TOML value | Written to sdkconfig.defaults |
| --- | --- | --- |
| bool | `true` / `false` | `CONFIG_X=y` / `# CONFIG_X is not set` |
| int | integer | `CONFIG_X=1000` |
| hex | `"0x…"` string | `CONFIG_X=0x9000` |
| string | string | `CONFIG_X="…"` |

- Keys must not include the `CONFIG_` prefix. Writing it is an error. Keys may contain only uppercase letters, digits, and underscores.
- Hex values must be written as `"0x…"` strings. Kconfig reads the number of a hex symbol as hexadecimal, so the integer `4096` is read as `0x4096`.
- When `[[partition]]` is present, `idf` appends the two partition-table settings at the end. A setting with the same name in `[config]` has no effect.

### choice

A choice (for example, the log level or the optimization level) cannot be assigned directly. Set the selected option to `true`:

```toml
[config]
LOG_DEFAULT_LEVEL_DEBUG = true     # Do not write LOG_DEFAULT_LEVEL = "debug"
```

Write only one option per choice. With `idf config set`, you can give the choice name and an option name. `idf` converts this to the form above and removes the other options of the same choice:

```bash
idf config set LOG_DEFAULT_LEVEL=debug
```

A choice option cannot be set to `false` on its own. To switch options, set a different one.

## config get

Print the effective value of a symbol in a format suitable for scripts:

```bash
idf config get FREERTOS_HZ             # 100
idf config get PARTITION_TABLE_OFFSET  # 0x8000
idf config get LOG_DEFAULT_LEVEL       # LOG_DEFAULT_LEVEL_INFO
```

A choice prints the name of the selected option. A hex value prints as `0x…`. A string prints without quotes. An inactive bool prints `false`. Other inactive symbols produce an error that states what they depend on and the current value of that dependency.

## config set

```bash
idf config set FREERTOS_HZ=1000 LOG_DEFAULT_LEVEL=debug
```

```text
         Set FREERTOS_HZ = 1000 in [config] (was 100)
         Set LOG_DEFAULT_LEVEL_DEBUG = true in [config] (was false)
```

- By default, the value is written to `[config]`. When `-r` or `--profile <name>` is given explicitly, it is written to `[profile.<name>.config]`:

  ```bash
  idf -r config set COMPILER_OPTIMIZATION=perf
  ```

  ```text
           Set COMPILER_OPTIMIZATION_PERF = true in [profile.release.config] (was false)
  ```

- The value is checked against the type and range from the menu before it is written. bool accepts `true`/`false`, `y`/`n`, `yes`/`no`, `on`/`off`, and `1`/`0`. int and hex accept decimal or `0x…`. Out-of-range values are an error.
- A symbol without a prompt is derived by Kconfig from other options. Writing it is rejected.
- Edits are made with toml_edit, which preserves comments. When an existing value is overwritten, its trailing comment is kept.
- After writing, kconfgen recalculates the result. If a value is set but has no effect, the reason is reported: the dependency is not enabled, the prompt is hidden, another option selects a different value, or the value is out of range.

## config diff

Lists the settable symbols whose values differ from the Kconfig defaults. The output is TOML and can be pasted directly into `idf.toml`:

```bash
idf config diff
```

```text
# dev for esp32s3: 2 set in idf.toml, 0 more that follow
FREERTOS_HZ = 1000              # default 100
LOG_DEFAULT_LEVEL_DEBUG = true  # instead of LOG_DEFAULT_LEVEL_INFO
```

The output has two groups: entries written in `idf.toml`, and entries that change as a consequence (`# Following from the settings above`). A choice lists only the selected option and notes which default option it replaces. Entries in `idf.toml` that equal the default are listed separately at the end (`# Already Kconfig's defaults: …`). They can be removed.

## config explain

```bash
idf config explain ESP_CONSOLE_UART_BAUDRATE
```

```text
ESP_CONSOLE_UART_BAUDRATE  (int, 1200..=4000000)
  menu:        Component config > ESP-STDIO > UART console baud rate
  value:       115200  (Kconfig default)
  default:     115200
  depends on:  ESP_CONSOLE_UART (ESP_CONSOLE_UART = y)
  settable if: ESP_CONSOLE_UART_CUSTOM (ESP_CONSOLE_UART_CUSTOM = n)
  defaults:    74880 if IDF_TARGET_ESP32C2 && XTAL_FREQ_26
               115200  <- applies
  defined at:  esp-idf/components/esp_stdio/Kconfig:150

  This baud rate is used by both the ESP-IDF Bootloader and the app ...
```

The output lists the type and range, the menu path, the current value and its source (`idf.toml [config]`, a profile, the Kconfig default, a select, or a value determined by other settings), the default, the dependencies (with the current values of the dependency symbols), the prompt condition, the sources of select and imply, multiple conditional defaults (with the one that applies marked), the Kconfig file where the symbol is defined, and the help text.

The example above shows why writing `ESP_CONSOLE_UART_BAUDRATE` alone has no effect. Its prompt is visible only when `ESP_CONSOLE_UART_CUSTOM` is enabled.

For a choice, `explain` lists all options, marks the selected one with `*`, and marks the default and its source:

```text
LOG_DEFAULT_LEVEL  (choice)
  menu:        Component config > Log > Log Level > Default log verbosity
    LOG_DEFAULT_LEVEL_NONE     No output
    LOG_DEFAULT_LEVEL_ERROR    Error
    LOG_DEFAULT_LEVEL_WARN     Warning
    LOG_DEFAULT_LEVEL_INFO     Info  (default)
  * LOG_DEFAULT_LEVEL_DEBUG    Debug  (idf.toml [config])
    LOG_DEFAULT_LEVEL_VERBOSE  Verbose
  set with:    idf config set LOG_DEFAULT_LEVEL=<OPTION>
  defined at:  esp-idf/components/log/Kconfig.level:3
```

## config menu

```bash
idf config menu
idf -r config menu     # changes are written to [profile.release.config]
```

Runs the upstream menuconfig and writes the differences back to `idf.toml` when you exit:

- menuconfig edits a sdkconfig generated only from `idf.toml`, stored in `build/<target>/<profile>/menuconfig/`. The build's own `sdkconfig` is not affected.
- Changed settable symbols are written back to `idf.toml`. For a choice, only the selected option is written.
- Entries that were set back to the Kconfig default are removed from the table, if removing them does not change the result. Otherwise the explicit value is kept.
- After writing back, the new `idf.toml` is recalculated. Symbols whose values do not match the menuconfig result produce a warning.
- A terminal is required. The command fails if stdin or stdout is not a terminal.

Several dozen lines of deprecation NOTEs from the SDK's own Kconfig are suppressed. Warnings and errors are still shown.

## Build-time checks

Each time `idf build` reconfigures CMake, it checks the configuration of the current profile and warns about the following:

- unknown keys (with similar names suggested);
- assignments to a choice itself (the warning states which option to write);
- symbols that have no prompt;
- values that are written but have no effect.

```text
warning: idf.toml: FREERTOS_HZZ is not a Kconfig option of this build (did you mean FREERTOS_HZ?)
warning: idf.toml: LOG_COLORS = "x" has no effect (false): Kconfig settled on another value
```

These are warnings only. The build continues.

## Implementation and cost

- The menu structure (types, prompts, help, ranges, dependencies) is read from `config/kconfig_menus.json`, which kconfgen generates in the build directory. Values are read from `config/sdkconfig.json`.
- Other values are computed by running kconfgen with the arguments from IDF's `kconfig.cmake`, using only defaults and no sdkconfig. This is the same result the next build gets from `idf.toml`. One run takes about 0.3 seconds.
- When only configuration values or the partition table change, the menu structure does not change, and the `config` command does not run the CMake configuration step (about 3 seconds). A new build directory must be configured once on the first `config` command.
- `kconfig_menus.json` does not contain `if` conditions without a prompt, the sources of select and imply, or multiple conditional defaults. When this information is needed (for `explain` and for the checks after `set`), `idf` runs a built-in script with the Python from the SDK venv. The script loads the Kconfig tree the same way kconfgen does and prints only the symbols that were asked for.

## Known limitations

::: warning
- `config set` checks only the value being set. Other values that change because an option was enabled are visible only in `config diff` for now.
- A symbol whose prompt is hidden (such as `ESP_CONSOLE_UART_BAUDRATE` above) is still written by `set`, with a warning only, because you may be about to enable its prerequisite at the same time.
:::
