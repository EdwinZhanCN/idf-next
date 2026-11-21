---
title: Build, flash, and monitor
description: Usage of build, clean, native, flash, monitor, run, erase, and image, and the rules for port detection, reset, baud rate, and reconnection.
---

# Build, flash, and monitor

## build

```bash
idf build                 # dev profile, chip from idf.toml
idf build -r              # release profile
idf build -t esp32c3      # use esp32c3 for this build only, without changing idf.toml
idf build --profile ci    # custom profile, must be declared as [profile.ci] in idf.toml
```

Each target and profile has one build directory: `build/<target>/<profile>/`. `idf build` performs the following steps in order:

1. Ensures the SDK is installed (writing `idf.lock` if necessary).
2. Generates the CMake project under `build/<target>/<profile>/project/`. Files whose content has not changed are not rewritten.
3. Runs the CMake configure step if generated files changed or `build.ninja` does not exist yet. Its output is hidden, and shown only on failure or with `-v`. Then it checks `[config]` (see [Configuration](/en/guide/config)) and merges the result of the component manager into `idf.lock`.
4. Runs `cmake --build`. Ninja output is passed through directly.
5. Links `compile_commands.json` in the project root to this build. If the project root already contains a real file with that name, it is left alone.

A full build of a small project takes about 25 to 30 seconds. A rebuild without code changes takes about 0.1 seconds (on an M-series Mac).

## clean

```bash
idf clean          # deletes build/<target>/<profile>/ for the current target and profile
idf clean --all    # deletes the whole build/ directory
```

In both cases, the stale `compile_commands.json` link is removed.

::: warning
The `--all` option of `idf clean --help` says "Also remove this project's entries from the global cache". The current implementation deletes only `build/` and does not touch `~/.idf-next/`.
:::

## native

`idf native` runs the upstream `idf.py` inside the managed environment. It serves as an escape hatch:

```bash
idf native size
idf native merge-bin
```

It runs `idf.py -C build/<t>/<p>/project -B build/<t>/<p> <args>`, using the same generated project and build directory as `idf build`. The exit code of `idf.py` is passed back unchanged. All arguments after `native` are passed to `idf.py`, so `idf`'s global options must be written before `native`.

## Serial port

`flash`, `run`, `monitor`, and `erase`, as well as `efuse` and `coredump`, need a serial port. The port is chosen in this order:

1. `-p, --port <PORT>`;
2. the environment variable `IDF_PORT`;
3. `port` in the user configuration file (see below);
4. automatic detection.

Automatic detection considers only USB serial ports, and recognizes these devices by USB VID/PID:

| Device | VID:PID |
| --- | --- |
| Espressif USB (USB-Serial-JTAG, USB-OTG CDC) | `303a:*` |
| CP210x | `10c4:ea60` |
| CH340 / CH343 / CH9102 | `1a86:7523` / `1a86:55d3` / `1a86:55d4` |
| FTDI FT232 / FT2232 / FT232H / FT231X | `0403:6001` / `6010` / `6014` / `6015` |

- On macOS, `/dev/tty.*` is removed and only `/dev/cu.*` is kept.
- If one port is found, it is used directly.
- If several are found, they are listed and you choose one in the terminal. With `--no-input`, or when not running in a terminal, an error is reported and the candidates are listed.
- If none is found, an error is reported. The other USB serial ports are listed so you can specify one with `-p`.

### User configuration

`~/.config/idf/config.toml` is optional and has only two keys:

```toml
port = "/dev/cu.usbmodem1101"   # used when neither -p nor IDF_PORT is given
flash_baud = 921600             # flash baud rate for flash, run, and erase when --baud is not given
```

The file location can be set with `$IDF_CONFIG`. Otherwise it is `$XDG_CONFIG_HOME/idf/config.toml`, and if that is not set, `~/.config/idf/config.toml`. Unknown keys are an error. The precedence is: command line, then environment variables, then the configuration file, then built-in defaults.

## flash

```bash
idf flash
idf flash --app                      # write the application only
idf flash --bootloader --partition-table
idf flash -b 921600
```

`flash` builds first, then writes every image listed in `flasher_args.json` in the build directory, including otadata and other images. `--app`, `--bootloader`, and `--partition-table` can be combined, in which case only the selected images are written.

- Images are written as they are. Like esptool, they are only padded to a multiple of 4 bytes. An image already on the board with identical content is skipped after an MD5 comparison and is shown as `Unchanged`.
- Before writing, the tool checks whether the chip ID and chip revision range in the image header match this board, and whether the image fits in the board's flash. If a check fails, the chip is reset to the original firmware and nothing is written.
- If the flash size in the build configuration is larger than the board's actual flash, only a warning is shown.
- The default flash baud rate is 460800. It can be changed with `-b` or with `flash_baud` in the configuration file.

::: tip ESP32-P4 earlier than v3
ESP-IDF builds for ESP32-P4 v3.x by default. If the chip revision is lower than v3, the revision check fails, and the error message suggests adding `ESP32P4_SELECTS_REV_LESS_V3 = true` to `[config]`. This option depends on the board.
:::

## monitor

```bash
idf monitor
idf monitor --no-reset    # attach to the running firmware without resetting
idf monitor -b 921600
```

- **Reset**: by default, the port is reset once after it is opened, so the output starts from the boot log (the same as `idf.py monitor`). `--no-reset` does not reset.
- **Baud rate**: taken from `ESP_CONSOLE_UART_BAUDRATE` in the build (or `ESPTOOLPY_MONITOR_BAUD` if that is absent), and 115200 if neither is present. `-b` overrides it. The `flash_baud` setting in the configuration file does not affect monitor.
- **Decoding**: the output is passed to espflash's parser. The application ELF and the ROM ELFs from `esp-rom-elfs` in the SDK are used to decode code addresses and RISC-V stack dumps. The ROM ELF chosen is the one with the highest version number that does not exceed the chip revision. When monitor runs by itself, the chip revision is taken from `ESP_REV_MIN_FULL` in sdkconfig. If the build has no ELF, a warning is shown and no decoding is done.
- **Outside a project**: monitor works, with a default baud rate of 115200, and no decoding. monitor does not install the SDK in order to get the ROM ELFs.
- **Keys**: Ctrl+R resets, Ctrl+C or Ctrl+] exits, and all other keys are sent to the board. Keys are handled only when both stdin and stdout are a terminal.
- **Colors**: when stdout is not a terminal, or when `NO_COLOR` is set, color escape sequences are removed.

### Reconnection after unplug

When a read error occurs on the port (the USB cable is unplugged, or the USB device re-enumerates), monitor waits for a port with the same name to reappear:

```text
--- /dev/cu.usbmodem1101 disconnected; waiting for it (Ctrl+C to exit) ---
--- /dev/cu.usbmodem1101 is back ---
```

This was tested on an ESP32-P4 (USB-Serial-JTAG): after the board is plugged back in, the connection is restored automatically, with no manual action needed. When the port reappears, the first few lines of ROM and bootloader output have already been printed, so the output after reconnection starts near `cpu_start`.

## run

```bash
idf run
```

Builds, flashes all images, and then opens monitor. After flashing, the chip is not reset. monitor opens the port, sets the baud rate, and then resets the chip once. This keeps the first lines of output from being lost on boards with a UART bridge, and prevents a second reset in the middle of boot on the ESP32-P4's USB-Serial-JTAG. `-b` affects only flashing. The monitor baud rate follows the rules in the previous section.

## erase

```bash
idf erase                          # erase the whole chip
idf erase --partition nvs          # erase one partition by name
idf erase --region 0x9000 24K      # erase SIZE bytes at OFFSET
```

- `--partition` reads the partition table from the board, and the table on the board is authoritative (the same as `parttool.py`). The table offset is taken from `PARTITION_TABLE_OFFSET` in the build, or 0x8000 if that is absent. If the name is not found, the partitions on the board are listed.
- The offset and size of `--region` must both be non-zero integer multiples of 4K (0x1000). They are checked before the tool connects to the board. The notation is the same as the size in `[[partition]]`: integers, `0x…`, `24K`, or `1M`.
- After erasing, the chip is reset into the application. This works outside a project too, and the chip is detected automatically.

## image

```bash
idf image                         # build/<t>/<p>/merged-binary.bin
idf image --format uf2            # build/<t>/<p>/uf2.bin
idf image --format dfu -o fw.dfu  # specify the output file
```

```text
       Wrote build/esp32s3/dev/merged-binary.bin (226.17 KiB, flash at 0x0)
```

`image` builds first, then merges the images listed in `flasher_args.json` into one file:

| Format | Upstream equivalent | Description |
| --- | --- | --- |
| `bin` | `esptool merge-bin` | A single image that is written starting at 0x0 |
| `uf2` | `esptool merge-bin --format uf2` | Uses the UF2 family ID of the chip |
| `dfu` | `mkdfu.py` | Supports only esp32s2, esp32s3, and esp32p4. The chunk size is taken from the environment variable `ESP_DFU_PART_SIZE` |

For the same ESP32-P4 build, the three formats are byte-identical to the output of `idf native merge-bin`, `uf2`, and `dfu`. DFU is also identical when `ESP_DFU_PART_SIZE=0x10000`. The esptool logic that rewrites the flash parameters in the bootloader header has not been ported. The parameters written into the header at build time are the same as those passed by `idf.py`, so the rewrite is a no-op.

## Known limitations

::: warning
- Device commands have been verified only on USB-Serial-JTAG (ESP32-P4 v1.3 development board, macOS arm64). The reset timing for UART bridge chips (CP210x, CH34x) follows espflash and has not been tested on hardware.
- monitor does not yet write logs to disk, add timestamps, or provide esp-idf-monitor's Ctrl+T menu.
- When `idf native … monitor` is killed, the child process of `idf.py` does not exit with it. (Interactive Ctrl+C is sent to the whole process group, so this does not affect that case.)
- After a change to sdkconfig, the bootloader subproject runs CMake again by itself, and its output appears in the output of `idf build`. (`idf.py` behaves the same way.)
:::
