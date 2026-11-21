---
title: Debugging and Security
description: "size, debug, coredump, efuse, secure: thin wrappers around upstream tools, automatically added arguments, eFuse burn protection, and the OpenOCD special case for ESP32-P4 before v3."
---

# Debugging and Security

`size`, `efuse`, `secure`, `coredump`, and `debug` are not implemented natively. They call upstream tools in the SDK environment and add the arguments `idf` already knows: the chip, the serial port, and the paths to the map and ELF files.

Common rules:

- The first four run `python -m <module> <args>` (`esp_idf_size`, `espefuse`, `espsecure`, `esp_coredump`). `debug` runs `idf.py openocd gdb`. The exit code of the upstream tool is returned unchanged.
- All arguments after the command name are passed to the upstream tool unchanged, including arguments such as `-p` and `-t` that have the same names as `idf` global options. Global `idf` options must be written before the command name, for example `idf -r size`.
- When you give an argument yourself, `idf` does not add it. The `--chip=esp32` form with an equals sign is also recognized.
- To see the help of the upstream tool, separate it with `--`: `idf size -- --help`, `idf efuse -- --help`.
- All of these must run inside a project. The SDK is installed first if needed. Add `-v` to see the command line that is actually executed.

## size

```bash
idf size                    # memory-type summary
idf size --archives         # per static library
idf size --format json2
idf -r size                 # build for the release profile
```

```text
                            Memory Type Usage Summary
┏━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━┳━━━━━━━━━━┳━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━┓
┃ Memory Type/Section ┃ Used [bytes] ┃ Used [%] ┃ Remain [bytes] ┃ Total [bytes] ┃
┡━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━╇━━━━━━━━━━╇━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━┩
│ Flash Code          │        65858 │          │                │               │
│ DIRAM               │        47953 │    14.03 │         293807 │        341760 │
│ ...                 │              │          │                │               │
```

If no `.map` file is among the arguments, `idf` adds `<project name>.map` from the build directory. `size` does not build first. If the current target and profile have not been built, the command fails and suggests running `idf build`. The content of `--format json2` matches `idf native size` (`idf.py` prints an extra Ninja preamble).

## efuse

```bash
idf efuse summary
idf efuse dump
idf efuse --virt summary    # virtual mode, does not connect to a chip
```

`idf` adds `--chip <the project's chip>` and `--port` chosen by the [serial port rules](/en/guide/build-flash-monitor). In `--virt` mode, or when help is requested, no port is added.

### Burn protection

eFuse burns are irreversible. A wrong value can make the chip unusable. Without `--virt` (that is, against a real chip), `idf` adds two protections before it connects to the chip:

1. **Burn subcommands require manual confirmation**: a prompt appears in the terminal, and you must type `burn` to continue. Any other input exits without burning anything. If stdin or stderr is not a terminal, the command is refused outright. Scripts and CI therefore cannot burn a real chip.
2. **`--do-not-confirm` is refused**: on a real chip, this argument is not passed to espefuse.

Subcommands treated as burn commands:

```text
burn-efuse  burn-block-data  burn-bit  burn-key  burn-key-digest
burn-custom-mac  set-flash-voltage  write-protect-efuse
read-protect-efuse  execute-scripts
```

Read-only subcommands are `summary`, `dump`, `get-custom-mac`, `adc-info`, and `check-error`.

These two lists are maintained by hand. The repository's `tests/efuse_commands.rs` reads the `--help` output of the installed espefuse for 12 chips. It requires every command in the "Burn commands" section to be in the burn list, and every command in the "Read commands" section to be in the read-only list. It also fails if an unknown section appears. When upstream adds a new burn command, this test fails first.

### Try --virt first

`--virt` is espefuse's virtual mode. It does not connect to a chip, and everything runs as usual, including virtual burns. Before a real burn, you can use it to check the command and arguments:

```bash
idf efuse --virt summary
```

```text
espefuse v5.6.0

=== Run "summary" command ===
EFUSE_NAME (Block) Description  = [Meaningful Value] [Readable/Writeable] (Hex Value)
----------------------------------------------------------------------------------------
Config fuses:
WR_DIS (BLOCK0)                                    Disable programming of individual eFuses           = 0 R/W (0x00000000)
...
```

::: warning Verification scope
In `--virt` mode, the output of `summary` is identical to calling espefuse directly, and virtual burns work. On a real chip, only the read-only `summary` has been run (on ESP32-P4). No eFuse has been burned.
:::

## secure

```bash
idf secure generate-signing-key --version 2 --scheme ecdsa256 signing_key.pem
idf secure generate-flash-encryption-key flash_key.bin
idf secure -- --help
```

Calls espsecure directly. It adds no arguments and does not touch the chip. Keys, signatures, and verification generated offline give the same results as calling espsecure directly.

## coredump

```bash
idf coredump info_corefile                                  # read from the board's flash
idf coredump info_corefile --core coredump.b64 --core-format b64
idf coredump dbg_corefile                                   # open with GDB
```

Arguments that `idf` adds:

- `--chip <the project's chip>`;
- when neither `--core`/`-c` (the dump file) nor `--port`/`-p` is given, the selected serial port, so the dump is read from flash;
- when no `.elf` file is given, the application ELF from the build is appended at the end. If the project has not been built, the command fails.

With no arguments, only `--chip` is added. esp-coredump then prints `Missing command.` and the usage.

::: warning
`coredump` has been checked only for command-line assembly. It has not yet been tested with a real dump.
:::

## debug

```bash
idf debug
```

Runs `idf.py openocd gdb`, using the same generated project and build directory as `idf build`. Arguments after `debug` are passed to `openocd gdb` in `idf.py`.

When connected to the chip's built-in USB-Serial-JTAG (USB `303a:1001`), `idf` sets the following automatically:

```text
OPENOCD_COMMANDS=-f board/<chip>-builtin.cfg
```

This requires that the SDK's OpenOCD scripts contain this board file, and that you have not set `OPENOCD_COMMANDS` yourself. If you have set it, your value is used as is. When the serial port is detected, no selection prompt is shown. If no port is found, or more than one board is connected, the variable is not set, and the SDK's default OpenOCD arguments are used.

### ESP32-P4 before v3

When the project selects `ESP32P4_SELECTS_REV_LESS_V3` (required for ESP32-P4 chips before v3; see [building, flashing, and monitoring](/en/guide/build-flash-monitor#flash)), `idf` also adds `-c 'gdb_memory_map disable'` and prints:

```text
   Debugging ESP32-P4 before v3: OpenOCD cannot read the flash map, so breakpoints in flash use the chip's hardware breakpoints
```

Reason: on ESP32-P4 v1.3, the OpenOCD flash stub triggers a PMP read access error at `0x43ff8000`, and GDB's connection is then refused. With the flash memory map disabled, GDB can connect. Breakpoints in flash (such as `app_main`) use hardware breakpoints instead.

## Known limitations

::: warning
- `debug` has been tried only on USB-Serial-JTAG. The P4 special case has been verified only on chip revision v1.3. JTAG adapters behind FTDI or other bridges still use the SDK's default OpenOCD arguments, and you must set `OPENOCD_COMMANDS` yourself.
- `coredump` has not been tested with a real dump.
- For `efuse`, only read-only commands have been run on a real chip, and only on an ESP32-P4 development board. Other chips are covered only by `--virt`.
:::
