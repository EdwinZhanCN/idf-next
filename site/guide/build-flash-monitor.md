---
title: 构建、烧录与监视
description: build、clean、native、flash、monitor、run、erase、image 的用法，端口检测、复位、波特率和重连的规则。
---

# 构建、烧录与监视

## build

```bash
idf build                 # dev profile，idf.toml 里的芯片
idf build -r              # release profile
idf build -t esp32c3      # 本次改用 esp32c3，不改 idf.toml
idf build --profile ci    # 自定义 profile，需在 idf.toml 里声明 [profile.ci]
```

每个目标和 profile 一个构建目录：`build/<target>/<profile>/`。`idf build` 依次：

1. 确保 SDK 已安装（必要时写 `idf.lock`）。
2. 生成 `build/<target>/<profile>/project/` 下的 CMake 工程，内容不变的文件不重写。
3. 有生成文件变化或还没有 `build.ninja` 时运行 CMake 配置（输出收起，失败或 `-v` 时显示），然后检查 `[config]`（见 [配置](/guide/config#构建时的检查)），并把组件管理器的结果合并进 `idf.lock`。
4. 运行 `cmake --build`，Ninja 输出直接透传。
5. 把项目根目录的 `compile_commands.json` 链接到这次构建。根目录已经有一个真实文件时不动它。

完整构建一个小项目约 25–30 秒，不改代码再次构建约 0.1 秒（M 系列 Mac）。

## clean

```bash
idf clean          # 删除当前目标和 profile 的 build/<target>/<profile>/
idf clean --all    # 删除整个 build/
```

两种情况下都会移除失效的 `compile_commands.json` 链接。

::: warning
`idf clean --help` 里 `--all` 的说明是 "Also remove this project's entries from the global cache"，但目前的实现只删除 `build/`，不碰 `~/.idf-next/`。
:::

## native

`idf native` 在托管环境里运行上游 `idf.py`，作为逃生口：

```bash
idf native size
idf native merge-bin
```

它执行的是 `idf.py -C build/<t>/<p>/project -B build/<t>/<p> <参数>`，与 `idf build` 使用同一个生成的工程和构建目录。`idf.py` 的退出码原样传回。`native` 之后的参数全部交给 `idf.py`，`idf` 的全局选项要写在 `native` 之前。

## 串口

`flash`、`run`、`monitor`、`erase` 以及 `efuse`、`coredump` 都要选一个串口。顺序是：

1. `-p, --port <PORT>`；
2. 环境变量 `IDF_PORT`；
3. 用户配置文件里的 `port`（见下文）；
4. 自动检测。

自动检测只看 USB 串口，按 USB VID/PID 认出这些设备：

| 设备 | VID:PID |
| --- | --- |
| Espressif USB（USB-Serial-JTAG、USB-OTG CDC） | `303a:*` |
| CP210x | `10c4:ea60` |
| CH340 / CH343 / CH9102 | `1a86:7523` / `1a86:55d3` / `1a86:55d4` |
| FTDI FT232 / FT2232 / FT232H / FT231X | `0403:6001` / `6010` / `6014` / `6015` |

- 在 macOS 上去掉 `/dev/tty.*`，只留 `/dev/cu.*`。
- 只找到一个：直接使用。
- 找到多个：在终端里列出来让你选；加了 `--no-input` 或不在终端里时报错并列出候选。
- 一个都没有：报错，并列出其他 USB 串口供你用 `-p` 指定。

### 用户配置

`~/.config/idf/config.toml` 是可选的，只有两个键：

```toml
port = "/dev/cu.usbmodem1101"   # 没有 -p 和 IDF_PORT 时使用
flash_baud = 921600             # 没有 --baud 时，flash、run、erase 的烧录波特率
```

文件位置可以用 `$IDF_CONFIG` 指定，否则是 `$XDG_CONFIG_HOME/idf/config.toml`，再否则是 `~/.config/idf/config.toml`。未知的键直接报错。优先级是命令行 > 环境变量 > 配置文件 > 内置默认。

## flash

```bash
idf flash
idf flash --app                      # 只写应用
idf flash --bootloader --partition-table
idf flash -b 921600
```

`flash` 先构建，再按构建目录里的 `flasher_args.json` 写入它列出的所有镜像（包括 otadata 等）。`--app`、`--bootloader`、`--partition-table` 可以组合，只写选中的那几个。

- 镜像原样写入，只像 esptool 一样补齐到 4 字节。板子上已有相同内容的镜像会按 MD5 跳过，显示为 `Unchanged`。
- 写入前检查：镜像头里的芯片 ID、芯片版本范围是否包含这块板子，以及镜像是否超出板子的 flash。检查失败时把芯片复位回原来的固件，不写入任何东西。
- 构建配置的 flash 大小大于板子实际大小时只给警告。
- 默认烧录波特率 460800，可用 `-b` 或配置文件的 `flash_baud` 修改。

::: tip ESP32-P4 v3 以下
ESP-IDF 默认按 ESP32-P4 v3.x 构建。芯片版本低于 v3 时版本检查会失败，错误提示会给出要加进 `[config]` 的 `ESP32P4_SELECTS_REV_LESS_V3 = true`。这是按板子决定的选项。
:::

## monitor

```bash
idf monitor
idf monitor --no-reset    # 直接接上正在运行的固件，不复位
idf monitor -b 921600
```

- **复位**：默认打开端口后复位一次，从启动日志开始看（与 `idf.py monitor` 相同）。`--no-reset` 不复位。
- **波特率**：取构建里的 `ESP_CONSOLE_UART_BAUDRATE`（没有时取 `ESPTOOLPY_MONITOR_BAUD`），都没有时 115200；`-b` 覆盖。配置文件的 `flash_baud` 不影响 monitor。
- **解码**：输出交给 espflash 的解析器，用应用 ELF 和 SDK 里 `esp-rom-elfs` 的 ROM ELF 解码代码地址和 RISC-V 栈转储。ROM ELF 取版本号不超过芯片版本的最大那个；单独运行 monitor 时芯片版本取 sdkconfig 的 `ESP_REV_MIN_FULL`。构建里没有 ELF 时给出警告，不解码。
- **项目外**：不在项目里也能用，波特率默认 115200，不解码。monitor 不会为了 ROM ELF 去安装 SDK。
- **按键**：Ctrl+R 复位，Ctrl+C 或 Ctrl+] 退出，其余按键发给板子。stdin 和 stdout 都是终端时才处理按键。
- **颜色**：stdout 不是终端或设置了 `NO_COLOR` 时去掉颜色序列。

### 拔插重连

端口读出错时（拔掉 USB，或 USB 设备重新枚举），monitor 等待同名端口回来：

```text
--- /dev/cu.usbmodem1101 disconnected; waiting for it (Ctrl+C to exit) ---
--- /dev/cu.usbmodem1101 is back ---
```

在 ESP32-P4（USB-Serial-JTAG）上实测过：插回后自动接上，不需要手动操作。端口重新出现时 ROM 和 bootloader 的开头几行已经输出完，所以重连后的输出从 `cpu_start` 附近开始。

## run

```bash
idf run
```

构建，烧录全部镜像，再打开 monitor。烧录结束时不复位；monitor 打开端口、设好波特率后复位一次。这样 UART 桥接的板子不会丢开头的输出，ESP32-P4 的 USB-Serial-JTAG 也不会在启动中途被第二次复位。`-b` 只影响烧录，monitor 的波特率按上一节的规则取。

## erase

```bash
idf erase                          # 整片擦除
idf erase --partition nvs          # 按名字擦一个分区
idf erase --region 0x9000 24K      # 擦 OFFSET 处的 SIZE 字节
```

- `--partition` 从板子上读分区表，以板子上的表为准（与 `parttool.py` 相同）。表的偏移取构建里的 `PARTITION_TABLE_OFFSET`，没有时 0x8000。找不到这个名字时列出板子上的分区。
- `--region` 的偏移和大小都必须是 4K（0x1000）的非零整数倍，在连接板子之前就检查。写法与 `[[partition]]` 的大小相同：整数、`0x…`、`24K`、`1M`。
- 擦除后复位进应用。不在项目里也能用，芯片自动识别。

## image

```bash
idf image                         # build/<t>/<p>/merged-binary.bin
idf image --format uf2            # build/<t>/<p>/uf2.bin
idf image --format dfu -o fw.dfu  # 指定输出文件
```

```text
       Wrote build/esp32s3/dev/merged-binary.bin (226.17 KiB, flash at 0x0)
```

`image` 先构建，再把 `flasher_args.json` 里的镜像合并成一个文件：

| 格式 | 对应上游 | 说明 |
| --- | --- | --- |
| `bin` | `esptool merge-bin` | 从 0x0 开始烧写的单个镜像 |
| `uf2` | `esptool merge-bin --format uf2` | 按芯片的 UF2 family ID |
| `dfu` | `mkdfu.py` | 只支持 esp32s2、esp32s3、esp32p4；分块大小认环境变量 `ESP_DFU_PART_SIZE` |

在同一个 ESP32-P4 构建上，三种格式与 `idf native merge-bin`、`uf2`、`dfu` 的输出逐字节一致；DFU 在 `ESP_DFU_PART_SIZE=0x10000` 时也一致。esptool 改写 bootloader 头部 flash 参数的逻辑没有移植：构建时写进头部的参数与 `idf.py` 传入的一样，改写是空操作。

## 已知限制

::: warning
- 设备命令只在 USB-Serial-JTAG（ESP32-P4 v1.3 开发板，macOS arm64）上验证过。UART 桥接芯片（CP210x、CH34x）的复位时序沿用 espflash 的，没有实测。
- monitor 还没有日志落盘、时间戳和 esp-idf-monitor 的 Ctrl+T 菜单。
- `idf native … monitor` 被杀掉时，`idf.py` 的子进程不会跟着退出（交互时 Ctrl+C 发给整个进程组，不受影响）。
- bootloader 子工程在 sdkconfig 变化后会自己重新运行一次 CMake，它的输出会出现在 `idf build` 的输出里（`idf.py` 也是这样）。
:::
