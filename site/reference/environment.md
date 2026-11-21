---
title: 环境变量与用户配置
description: idf 读取的环境变量、~/.config/idf/config.toml 的键、优先级，以及 idf env 输出的变量。
---

# 环境变量与用户配置

机器相关的设置（串口、烧录波特率、缓存位置）不写进 `idf.toml`，而是来自命令行、环境变量或用户配置文件。本页列出 `idf` 读取的全部环境变量（取自 `src/` 中的 `env::var` 调用）、用户配置文件的键，以及 `idf` 传给上游工具的环境。

## 优先级

同一项设置有多个来源时，优先级从高到低：

1. 命令行选项；
2. 环境变量；
3. 用户配置文件 `~/.config/idf/config.toml`；
4. 内置默认。

| 设置 | 命令行 | 环境变量 | 用户配置 | 内置默认 |
| --- | --- | --- | --- | --- |
| 串口 | `-p, --port` | `IDF_PORT` | `port` | 按 USB VID/PID 自动探测 |
| 烧录波特率（`flash`、`run`、`erase`） | `-b, --baud` | 无 | `flash_baud` | 460800 |
| 监视器波特率（`monitor`） | `-b, --baud` | 无 | 无 | sdkconfig 的 `ESP_CONSOLE_UART_BAUDRATE`，其次 `ESPTOOLPY_MONITOR_BAUD`，最后 115200 |
| 全局缓存位置 | 无 | `IDF_NEXT_HOME` | 无 | `~/.idf-next` |
| 用户配置文件位置 | 无 | `IDF_CONFIG`，其次 `XDG_CONFIG_HOME` | 无 | `~/.config/idf/config.toml` |

`idf run -b` 只影响烧录；`run` 里的监视器总是用 sdkconfig 的波特率。

## 用户配置文件 {#user-config}

```toml
# ~/.config/idf/config.toml
port = "/dev/cu.usbmodem1101"
flash_baud = 921600
```

| 键 | 类型 | 说明 |
| --- | --- | --- |
| `port` | 字符串 | 串口。没有 `-p` 和 `IDF_PORT` 时使用，对所有用到串口的命令生效。 |
| `flash_baud` | 整数 | 烧录波特率。没有 `--baud` 时用于 `flash`、`run`、`erase`。不影响 `monitor`。 |

- 文件是可选的，不存在时等于空配置。
- 只有这两个键。未知的键直接报错（错误信息以 `parsing <文件路径>` 开头），避免拼错的键被悄悄忽略。
- 除 `--help` 和 `--version` 外，每个命令启动时都会读取这个文件，所以文件有错时所有命令都会失败。

文件位置按顺序取第一个可用的：

1. `$IDF_CONFIG`（文件的完整路径）；
2. `$XDG_CONFIG_HOME/idf/config.toml`；
3. `~/.config/idf/config.toml`。

空字符串视为未设置。

## idf 读取的环境变量

### 设备与输出

| 变量 | 作用 |
| --- | --- |
| `IDF_PORT` | 串口，等同 `-p`。`-p` 优先。 |
| `NO_COLOR` | 设置了（任何值）就不输出颜色：状态行和警告不上色，`monitor` 去掉固件输出里的颜色序列，logo 不上色。 |
| `OPENOCD_COMMANDS` | `idf debug` 传给 OpenOCD 的参数。设置了就原样使用；没设置时，`idf debug` 在检测到芯片内置的 USB-Serial-JTAG 时自动设置，见 [命令](/reference/commands#idf-debug)。 |
| `ESP_DFU_PART_SIZE` | `idf image --format dfu` 的分块大小，十进制或 `0x` 开头的十六进制。未设置时为 512 KB，与 `mkdfu.py` 的默认值相同。 |

### 缓存与工具定位

| 变量 | 作用 |
| --- | --- |
| `IDF_NEXT_HOME` | 全局缓存的根目录，默认 `~/.idf-next`。见 [全局缓存](#home)。 |
| `IDF_CONFIG` | 用户配置文件的路径。 |
| `XDG_CONFIG_HOME` | 用户配置文件的基目录，取 `$XDG_CONFIG_HOME/idf/config.toml`。 |
| `IDF_NEXT_UV` | 用来创建 SDK venv 的 uv 可执行文件。设置了就优先使用；否则用与 `idf` 真实路径同目录的 `uv`（`uv tool install idf-next` 安装的就是这样）；再否则用 `PATH` 里的 `uv`。 |

### 网络

| 变量 | 作用 |
| --- | --- |
| `GITHUB_TOKEN` | 查询 ESP-IDF 发布版本时用于 GitHub API 认证，避免匿名请求的频率限制。遇到 403 或 429 时，错误信息会提示设置它。 |
| `IDF_GITHUB_ASSETS` | 工具链下载的镜像，含义与 `idf_tools.py` 相同：`https://github.com/...` 改写为 `https://$IDF_GITHUB_ASSETS/...`。 |

### 终端 logo

以下变量只影响 `idf`、`idf --help`、`idf --version` 在终端里画的 logo，不影响其他输出：

| 变量 | 作用 |
| --- | --- |
| `TERM` | `dumb` 时不画；`linux`、`ansi` 或 `vt*` 时只用红色。 |
| `COLORTERM` | `truecolor` 或 `24bit` 时用 24 位色，否则用 256 色。 |
| `LC_ALL`、`LC_CTYPE`、`LANG` | locale 不是 UTF-8 时退回 ASCII 字符。 |

stdout 不是终端或使用 `-q` 时也不画。管道和脚本拿到的输出与没有 logo 时完全相同。

## 全局缓存 {#home}

`IDF_NEXT_HOME`（默认 `~/.idf-next`）下的布局：

```text
~/.idf-next/
  sdk/<version>/                  一个 SDK，作为整体安装和删除
    esp-idf/                      源码（没有 .git，补了 version.txt）
    tools/<tool>/<version>/       来自 SDK tools.json 的工具链
    python/<摘要前 16 位>/         每个 Python 解析结果一个 venv
      venv/
      espidf.constraints.vX.Y.txt
      requirements.txt
      env.toml                    最后写入：标记 venv 完整
    sdk.toml                      最后写入：标记源码和工具完整
  downloads/                      按哈希寻址的下载存储（离线重装用）
  python/                         uv 管理的 Python 解释器
  components/                     组件管理器的下载缓存，各项目共用
  uv-cache/                       uv 的 wheel 缓存（以硬链接复用进 venv）
  .lock                           SDK 安装和删除时持有的文件锁
```

`idf sdk remove` 只删除 `sdk/<version>/`。`downloads/`、`python/`、`uv-cache/` 保留，所以可以 `--offline` 重装。

## idf 传给上游工具的环境 {#upstream}

`idf` 运行 CMake、Ninja、`idf.py` 和其他上游 Python 工具时，在当前环境的基础上设置下面这些变量。不需要激活任何环境，也不需要 `export.sh`。

### SDK 环境

由 `idf env` 输出（见下一节），所有调用上游工具的命令都会设置：

| 变量 | 值 |
| --- | --- |
| `IDF_PATH` | `~/.idf-next/sdk/<version>/esp-idf` |
| `IDF_TOOLS_PATH` | Python 环境目录 `~/.idf-next/sdk/<version>/python/<摘要前 16 位>`，里面放着 `espidf.constraints.vX.Y.txt` |
| `IDF_PYTHON_ENV_PATH` | 该目录下的 `venv` |
| `ESP_IDF_VERSION` | 发行系列，如 `6.0` |
| 工具导出的变量 | 来自 `tools.json` 的 `export_vars`，例如 `OPENOCD_SCRIPTS`、`ESP_ROM_ELF_DIR` |
| `PATH` | 前面依次加上 venv 的 `bin`、各工具的 `export_paths`、`esp-idf/tools` |

`IDF_TOOLS_PATH` 和 `IDF_PYTHON_ENV_PATH` 指向托管的 venv 和 constraints 文件，所以 IDF 自己的 `idf_tools.py check-python-dependencies`（CMake 每次构建前都会跑）原样通过。

### 构建目录相关

构建、配置和薄封装命令额外设置：

| 变量 | 值 | 原因 |
| --- | --- | --- |
| `IDF_TARGET` | 当前芯片 | 选择 target |
| `IDF_SKIP_CHECK_SUBMODULES` | `1` | SDK 里没有 `.git`，没有子模块可检查 |
| `IDF_COMPONENT_MANAGER` | 有依赖时 `1`，否则 `0` | 没有依赖时不让组件管理器拖慢配置 |
| `IDF_COMPONENT_CACHE_PATH` | `~/.idf-next/components` | 组件下载缓存在各项目间共用 |

CMake 配置时另外传入 `-DPYTHON=<venv 里的 python>`、`-DIDF_TARGET=<芯片>` 和 `-DCCACHE_ENABLE=0`。

### uv

调用 uv 时设置 `UV_CACHE_DIR=~/.idf-next/uv-cache`、`UV_PYTHON_INSTALL_DIR=~/.idf-next/python`、`UV_NO_CONFIG=1`，并移除 `VIRTUAL_ENV`、`UV_PYTHON`、`UV_INDEX_URL`、`UV_EXTRA_INDEX_URL`。也就是说，用户自己的 uv 配置和激活的 venv 不会影响 SDK venv 的创建。

## idf env 输出的变量 {#idf-env}

`idf env` 打印上面的 SDK 环境。需要在项目里运行；SDK 未安装时会先安装，没有 lock 时写入 lock。

```bash
$ idf env
IDF_PATH             ~/.idf-next/sdk/6.0.3/esp-idf
IDF_TOOLS_PATH       ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c
IDF_PYTHON_ENV_PATH  ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv
ESP_IDF_VERSION      6.0
OPENOCD_SCRIPTS      ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/v0.12.0-esp32-20260703/openocd-esp32/share/openocd/scripts
ESP_ROM_ELF_DIR      ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528/
PATH                 ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf-gdb/17.1_20260402/xtensa-esp-elf-gdb/bin
                     ~/.idf-next/sdk/6.0.3/tools/riscv32-esp-elf-gdb/17.1_20260402/riscv32-esp-elf-gdb/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf/esp-15.2.0_20251204/xtensa-esp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/riscv32-esp-elf/esp-15.2.0_20251204/riscv32-esp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/esp32ulp-elf/2.38_20240113/esp32ulp-elf/bin
                     ~/.idf-next/sdk/6.0.3/tools/cmake/4.0.3/CMake.app/Contents/bin
                     ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/v0.12.0-esp32-20260703/openocd-esp32/bin
                     ~/.idf-next/sdk/6.0.3/tools/ninja/1.12.1
                     ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528
                     ~/.idf-next/sdk/6.0.3/esp-idf/tools
```

（macOS arm64、ESP-IDF 6.0.3。实际输出是绝对路径，这里把主目录写成 `~`。）

`--shell` 输出 POSIX shell 的 `export` 语句，值用单引号包起来，`PATH` 接在原有 `PATH` 前面（以下有删节）：

```bash
$ idf env --shell
export IDF_PATH='/Users/me/.idf-next/sdk/6.0.3/esp-idf'
export IDF_TOOLS_PATH='/Users/me/.idf-next/sdk/6.0.3/python/14645654ec2ff38c'
...
export PATH='/Users/me/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin:...:/Users/me/.idf-next/sdk/6.0.3/esp-idf/tools:'"${PATH}"
```

在当前 shell 里使用这些工具：

```bash
eval "$(idf env --shell)"
```

::: tip
日常使用不需要 `idf env`：`idf` 的每个命令都会自己设置好环境。它用于在 `idf` 之外直接调用工具链，例如在编辑器或脚本里运行 `xtensa-esp-elf-gdb`。
:::
