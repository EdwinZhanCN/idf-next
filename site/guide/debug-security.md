---
title: 调试与安全
description: size、debug、coredump、efuse、secure：上游工具的薄封装，自动补齐的参数，eFuse 烧写保护，以及 ESP32-P4 v3 以下的 OpenOCD 特例。
---

# 调试与安全

`size`、`efuse`、`secure`、`coredump`、`debug` 不是原生实现，而是在 SDK 的环境里调用上游工具，由 `idf` 补上它已经知道的参数：芯片、串口、map 和 ELF 的路径。

共同的规则：

- 前四个执行 `python -m <模块> <参数>`（`esp_idf_size`、`espefuse`、`espsecure`、`esp_coredump`），`debug` 执行 `idf.py openocd gdb`。上游的退出码原样传回。
- 命令名之后的所有参数原样交给上游工具，包括 `-p`、`-t` 这类与 `idf` 全局选项同名的参数。`idf` 的全局选项要写在命令名之前，例如 `idf -r size`。
- 某个参数已经由你给出时，`idf` 不再补它（`--chip=esp32` 这种等号写法也能识别）。
- 看上游工具自己的帮助，用 `--` 隔开：`idf size -- --help`、`idf efuse -- --help`。
- 都需要在项目里运行，需要时会先安装 SDK。加 `-v` 可以看到实际执行的命令行。

## size

```bash
idf size                    # 内存类型汇总
idf size --archives         # 按静态库统计
idf size --format json2
idf -r size                 # release profile 的构建
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

参数里没有 `.map` 文件时，`idf` 补上构建目录里的 `<项目名>.map`。`size` 不会先构建；当前目标和 profile 还没构建过时报错，提示先运行 `idf build`。`--format json2` 的内容与 `idf native size` 一致（`idf.py` 会多打印一段 Ninja 的前言）。

## efuse

```bash
idf efuse summary
idf efuse dump
idf efuse --virt summary    # 虚拟模式，不连芯片
```

`idf` 补上 `--chip <项目的芯片>`，以及按 [串口规则](/guide/build-flash-monitor#串口) 选出的 `--port`。`--virt` 或查看帮助时不补端口。

### 烧写保护

eFuse 烧写不可逆，写错值可能让芯片无法使用。不带 `--virt`（即面对真芯片）时，`idf` 在连接芯片之前加了两道保护：

1. **烧写类子命令必须手动确认**：在终端里提示，必须输入 `burn` 才继续；输入其他内容就退出，什么都不烧。stdin 或 stderr 不是终端时直接拒绝，所以脚本和 CI 里无法烧写真芯片。
2. **拒绝 `--do-not-confirm`**：真芯片上不会把这个参数传给 espefuse。

被视为烧写类的子命令：

```text
burn-efuse  burn-block-data  burn-bit  burn-key  burn-key-digest
burn-custom-mac  set-flash-voltage  write-protect-efuse
read-protect-efuse  execute-scripts
```

只读的子命令是 `summary`、`dump`、`get-custom-mac`、`adc-info`、`check-error`。

这两份名单是手写的。仓库里的 `tests/efuse_commands.rs` 读取已安装的 espefuse 对 12 个芯片的 `--help`，要求 “Burn commands” 一栏全在烧写名单里、“Read commands” 一栏全在只读名单里，出现没见过的栏也失败。上游加了新的烧写命令时，这个测试会先失败。

### 先用 --virt 试

`--virt` 是 espefuse 的虚拟模式，不连芯片，一切照常执行，包括虚拟的烧写。真正烧写之前，可以先用它确认命令和参数：

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

::: warning 验证范围
`--virt` 模式下 `summary` 的输出与直接调用 espefuse 逐字一致，虚拟烧写可用。真芯片上只运行过只读的 `summary`（ESP32-P4），没有烧过任何 eFuse。
:::

## secure

```bash
idf secure generate-signing-key --version 2 --scheme ecdsa256 signing_key.pem
idf secure generate-flash-encryption-key flash_key.bin
idf secure -- --help
```

直接调用 espsecure，不补任何参数，也不碰芯片。离线生成密钥、签名、验证的结果与直接调用相同。

## coredump

```bash
idf coredump info_corefile                                  # 从板子的 flash 读取
idf coredump info_corefile --core coredump.b64 --core-format b64
idf coredump dbg_corefile                                   # 用 GDB 打开
```

`idf` 补的参数：

- `--chip <项目的芯片>`；
- 参数里没有 `--core`/`-c`（转储文件）也没有 `--port`/`-p` 时，补上选中的串口，转储从 flash 读取；
- 参数里没有 `.elf` 文件时，在末尾补上构建里的应用 ELF。还没构建时报错。

不带任何参数时只补 `--chip`，esp-coredump 报 `Missing command.` 并打印用法。

::: warning
`coredump` 只验证了命令行的拼装，还没有用真实的转储解码过。
:::

## debug

```bash
idf debug
```

执行 `idf.py openocd gdb`，使用与 `idf build` 相同的生成工程和构建目录。`debug` 之后的参数接在 `openocd gdb` 后面交给 `idf.py`。

连接的是芯片内置的 USB-Serial-JTAG（USB `303a:1001`）时，`idf` 自动设置：

```text
OPENOCD_COMMANDS=-f board/<芯片>-builtin.cfg
```

前提是 SDK 的 OpenOCD 脚本里有这个板级文件，并且你没有自己设置 `OPENOCD_COMMANDS`；设置了就原样使用。探测串口时不会提示选择，找不到或有多块板子时就不设，交给 SDK 默认的 OpenOCD 参数。

### ESP32-P4 v3 以下

项目选了 `ESP32P4_SELECTS_REV_LESS_V3`（P4 v3 以下的芯片必须选，见 [构建、烧录与监视](/guide/build-flash-monitor#flash)）时，`idf` 再加上 `-c 'gdb_memory_map disable'`，并输出：

```text
   Debugging ESP32-P4 before v3: OpenOCD cannot read the flash map, so breakpoints in flash use the chip's hardware breakpoints
```

原因：在 P4 v1.3 上，OpenOCD 的 flash 桩程序会在 `0x43ff8000` 触发 PMP 读访问错误，GDB 的连接随之被拒绝。关掉 flash 内存映射后 GDB 可以连接，flash 里的断点（如 `app_main`）改用硬件断点。

## 已知限制

::: warning
- `debug` 只在 USB-Serial-JTAG 上试过，P4 的特例只在芯片版本 v1.3 上验证过。FTDI 或其他桥接的 JTAG 适配器仍用 SDK 默认的 OpenOCD 参数，需要自己设置 `OPENOCD_COMMANDS`。
- `coredump` 没有用真实转储测过。
- `efuse` 在真芯片上只在 ESP32-P4 开发板上运行过只读命令，其他芯片只有 `--virt` 覆盖。
:::
