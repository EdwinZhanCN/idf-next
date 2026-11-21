---
title: 命令
description: idf 的全部命令、参数和全局选项。
---

# 命令

本页列出 `idf` 0.1.0 的全部命令。选项名和说明以 `idf <命令> --help` 为准，行为说明以源码为准。

```text
Usage: idf [OPTIONS] <COMMAND>
```

命令分四组：

| 分组 | 命令 |
| --- | --- |
| 项目 | [`new`](#idf-new)、[`init`](#idf-init)、[`add`](#idf-add)、[`remove`](#idf-remove)、[`update`](#idf-update)、[`tree`](#idf-tree)、[`target`](#idf-target)、[`config`](#idf-config) |
| 构建与设备 | [`build`](#idf-build)、[`run`](#idf-run)、[`flash`](#idf-flash)、[`monitor`](#idf-monitor)、[`erase`](#idf-erase)、[`image`](#idf-image)、[`size`](#idf-size)、[`clean`](#idf-clean) |
| 调试与安全 | [`debug`](#idf-debug)、[`coredump`](#idf-coredump)、[`efuse`](#idf-efuse)、[`secure`](#idf-secure) |
| 环境 | [`sdk`](#idf-sdk)、[`env`](#idf-env)、[`doctor`](#idf-doctor)、[`native`](#idf-native)、[`completions`](#idf-completions) |

## 全局选项

下面的选项对所有命令都可用，可以写在子命令之前或之后（`idf -r build` 与 `idf build -r` 等价）。并不是每个命令都会用到每个选项，例如 `-p` 只对连接串口的命令有意义。

| 选项 | 说明 |
| --- | --- |
| `-C, --project <PATH>` | 当作在 `PATH` 中启动。从 `PATH` 开始向上逐级查找 `idf.toml`；`PATH` 必须是已存在的目录。`init` 直接在 `PATH` 初始化。 |
| `-t, --target <CHIP>` | 仅本次调用使用的芯片，覆盖 `idf.toml` 的 `target`。决定使用哪个构建目录 `build/<target>/<profile>/`。对 `new` 和 `init`，它是写进新 `idf.toml` 的芯片。芯片名的校验规则见 [idf.toml](/reference/idf-toml#project)。 |
| `-p, --port <PORT>` | 串口。可由环境变量 `IDF_PORT` 提供；两者都没有时读用户配置的 `port`，再没有就按 USB VID/PID 自动探测。见 [串口选择](#port)。 |
| `-r, --release` | 使用 `release` profile。与 `--profile` 互斥。 |
| `--profile <NAME>` | 使用指定的 profile。`dev` 和 `release` 是内置的，其他名字必须在 `idf.toml` 里以 `[profile.<NAME>]` 声明，否则报错 `no profile named ...`。不指定时为 `dev`。 |
| `--offline` | 不访问网络。`idf` 自己的下载和 GitHub 查询会直接报错，调用 uv 时也带上离线参数。 |
| `--locked` | 需要修改 `idf.lock` 时报错，而不是修改它。详见 [idf.lock](/reference/idf-lock#locked)。 |
| `--no-input` | 不提示输入。目前只影响串口选择：找到多块板子时直接报错并列出候选。 |
| `-v, --verbose...` | 更多输出：显示 CMake 配置阶段的输出、执行的命令行、网络重试、组件管理器的完整输出等。可以重复，但当前实现只区分「有没有 `-v`」。与 `-q` 互斥。 |
| `-q, --quiet` | 更少输出：不打印状态行和进度条，也不画 logo。警告和错误照常输出。 |
| `-h, --help` | 打印帮助。 |
| `-V, --version` | 打印版本。`-V` 只输出一行 `idf 0.1.0`；`--version` 另起一行输出支持的 ESP-IDF 范围。 |

```bash
$ idf --version
idf 0.1.0
supported ESP-IDF: >=6.0.0, <6.1.0
```

::: details --offline 不覆盖组件管理器
`--offline` 约束的是 `idf` 本身的网络访问（SDK 源码、工具链、Python 解释器和包、GitHub API）。CMake 配置阶段调用的 ESP-IDF 组件管理器不会收到这个选项。全新构建目录第一次配置有依赖的项目时，组件管理器仍需要联网读取注册表元数据，见 [路线图与现状](/design/status)。
:::

### 串口选择 {#port}

用到串口的命令（`run`、`flash`、`monitor`、`erase`，以及 `efuse`、`coredump` 自动补端口时）按以下顺序取端口：

1. `-p/--port`；
2. 环境变量 `IDF_PORT`；
3. 用户配置 `~/.config/idf/config.toml` 的 `port`（见 [环境变量与用户配置](/reference/environment)）；
4. 自动探测：列出所有 USB 串口，只保留已知的 USB VID/PID（Espressif `303a:*`、CP210x、CH340、CH343、CH9102、FTDI FT232/FT2232/FT232H/FT231X）。macOS 上忽略 `/dev/tty.*`，只用 `/dev/cu.*`。
   - 恰好一个：直接使用；
   - 一个都没有：报错 `no ESP board found on USB`，并列出其他 USB 串口；
   - 多个：在终端里提示选择；设置了 `--no-input` 或不在终端里时报错 `several boards found: ...`。

### 需要项目和 SDK 的命令 {#needs-project}

| 情况 | 命令 |
| --- | --- |
| 不需要项目 | `new`、`completions`、`sdk install <版本>`、`sdk list`、`sdk remove`、`sdk path <版本>` |
| 项目可选 | `init`（在当前目录或 `-C` 目录创建）、`target list`、`monitor`、`erase`、`doctor` |
| 需要项目，不需要 SDK | `target set`、`clean` |
| 需要项目，并在需要时自动安装 SDK、写入 `idf.lock` | `build`、`run`、`flash`、`image`、`size`、`config`、`add`、`remove`、`update`、`tree`、`debug`、`coredump`、`efuse`、`secure`、`native`、`env`、`sdk install`（不带版本）、`sdk use` |

找不到项目时报错：

```text
`idf build` needs a project, but no idf.toml was found here or in any parent directory
```

## 项目

### idf new

在新目录中创建项目。

```text
idf new [OPTIONS] <PATH>
```

| 参数 | 说明 |
| --- | --- |
| `<PATH>` | 新项目目录，必须不存在。目录名就是项目名。 |

- 生成 `idf.toml`、`src/main.c` 和 `.gitignore`（内容为 `/build/` 和 `/compile_commands.json`）。
- 芯片取 `-t`，默认 `esp32`；`idf` 字段写 `"6.0"`。
- 目录名必须是合法的项目名（字母开头，只含字母、数字、`-`、`_`），否则报错 `` `<name>` cannot be used as a project name ``。
- 目录已存在时报错 `<PATH> already exists`，提示改用 `idf init`。
- 不访问网络，不安装 SDK，不写 `idf.lock`。`<PATH>` 相对当前工作目录解析。

```bash
idf new blink -t esp32s3
```

生成的 `idf.toml`：

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"
```

### idf init

在当前目录（或 `-C` 指定的目录）创建项目。

```text
idf init [OPTIONS]
```

- 生成的内容与 `new` 相同。
- 目录里已有 `idf.toml` 时报错 `<dir> already has an idf.toml`。
- `src/` 已存在且非空时不写 `src/main.c`；`.gitignore` 已存在时不覆盖。
- 项目名取目录名，规则同 `new`。

### idf add

添加依赖，写入 `idf.toml` 的 `[dependencies]`，并把解析结果写入 `idf.lock`。

```text
idf add [OPTIONS] <SPECS>...
```

| 参数和选项 | 说明 |
| --- | --- |
| `<SPECS>...` | `namespace/name[@requirement]`，或配合 `--git` / `--path` 时的依赖名。 |
| `--git <GIT>` | 从 git 仓库取组件（仓库 URL）。与 `--path` 互斥。 |
| `--rev <REV>` | git 仓库的分支、tag 或提交。需要 `--git`。 |
| `--path <PATH>` | 从本地目录取组件，依赖以目录名命名。 |

- 注册表依赖没写要求时，先用 `*` 求解一次，再把 `^<解析到的版本>` 写进 `idf.toml`；写了 `@要求` 就原样写入。
- `--git` 和 `--path` 只接受一个依赖名。
- 路径依赖必须以组件目录名命名，因为组件管理器按目录名查找组件；不一致时报错并给出正确写法。路径在项目内时写成相对项目根目录的路径。
- 求解由组件管理器完成（`idf_component_manager.prepare_components`），不运行 CMake。需要联网。
- 完成后删除构建目录里的 `build.ninja`，下一次构建会重新运行 CMake 配置，让 CMake 看到新的组件集合。`remove` 和 `update` 也是如此。
- `--locked` 时直接报错：`` `idf add` would change idf.toml and idf.lock, but --locked was given ``。
- 新增的条目不带 `targets` 和 `idf` 条件；需要条件依赖时手动编辑 `idf.toml`，见 [idf.toml](/reference/idf-toml#dependencies)。

```bash
idf add espressif/led_strip
idf add espressif/button@^4.1
idf add mylib --git https://example.com/mylib.git --rev v1.0
idf add mylib --path ../mylib
```

### idf remove

删除依赖，并清掉 `idf.lock` 里不再需要的条目。

```text
idf remove [OPTIONS] <NAMES>...
```

- 名字必须是 `[dependencies]` 里已有的键，否则报错并列出现有依赖。
- 删除后重新求解并更新 `idf.lock`。`--locked` 时报错。

### idf update

在 `idf.toml` 的约束范围内更新 `idf.lock`。

```text
idf update [OPTIONS] [NAMES]...
```

| 参数 | 说明 |
| --- | --- |
| `[NAMES]...` | 只更新这些依赖（默认全部）。 |

- 不带名字时丢掉 `idf.lock` 里全部组件锁；带名字时丢掉这些依赖，以及只被它们带进来的间接依赖。然后重新求解。
- 打印版本变化：`Updated <name> <旧> -> <新>`、`Added <name> <版本>`，没有变化时 `Unchanged everything is up to date`。
- `--locked` 时报错。
- 只更新组件依赖，不更新 ESP-IDF 版本；升级 SDK 用 [`idf sdk use`](#idf-sdk-use)。

### idf tree

打印组件依赖树。

```text
idf tree [OPTIONS]
```

- 优先读取上次配置时组件管理器的求解结果；没有就先求解。`--offline` 下没有结果时报错。
- 第一行是 `<项目名> (ESP-IDF <版本>)`。重复出现且有子依赖的组件标 `(*)`，不再展开；没解析到的标 `(not resolved)`。
- 项目没有 `[dependencies]` 时只打印一行提示。

### idf target

设置或列出芯片。

```text
idf target [OPTIONS] <COMMAND>
```

#### idf target set

```text
idf target set [OPTIONS] <CHIP>
```

把 `target` 写进 `idf.toml`，保留文件里的注释和格式。芯片名不在已知列表里时给出警告，交给 SDK 判断。

#### idf target list

```text
idf target list [OPTIONS]
```

列出已知芯片（ESP-IDF v6.0 的非预览芯片）。在项目里运行时，当前项目的芯片后面标 `(this project)`。

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

读取和修改 Kconfig 配置。所有子命令都作用于当前 target 和 profile 的构建目录，配置只写进 `idf.toml`，不会产生项目目录里的 sdkconfig。

```text
idf config [OPTIONS] <COMMAND>
```

- 构建目录还没有配置过，或者源文件、组件、Kconfig 文件有变化时，会先运行一次 CMake 配置（约 3 秒）。只有 `[config]` 或分区表变化时不运行 CMake，直接用 kconfgen 计算（约 0.3 秒）。
- 值的计算方式与一次构建相同：只用 `idf.toml` 生成的 defaults，不用旧的 sdkconfig。

用法说明见 [配置](/guide/config)。

#### idf config get

```text
idf config get [OPTIONS] <KEY>
```

打印一个符号的生效值，便于脚本使用：

- `KEY` 可以带或不带 `CONFIG_` 前缀；
- choice 打印选中的选项名；hex 打印 `0x…`；字符串不加引号；bool 打印 `true` / `false`；
- 未激活的 bool 打印 `false`；其他未激活的符号报错，并说明它依赖什么、依赖符号当前的值；
- 名字不存在时报错，并给出相近的名字。

#### idf config set

```text
idf config set [OPTIONS] <ASSIGNMENTS>...
```

| 参数 | 说明 |
| --- | --- |
| `<ASSIGNMENTS>...` | `KEY=VALUE` 对。 |

- 写入 `[config]`；用 `-r` 或 `--profile` 显式选了 profile 时写入 `[profile.<name>.config]`。
- 按 Kconfig 的类型和范围检查：bool 接受 `true`、`y`、`yes`、`on`、`1` 和 `false`、`n`、`no`、`off`、`0`；int 和 hex 接受十进制或 `0x` 开头的十六进制；超出 `range` 时报错。hex 值写成字符串 `"0x…"`。
- choice 写成 `CHOICE=OPTION`，`OPTION` 可以是完整的选项名或去掉 choice 名前缀的部分，不区分大小写。例如 `LOG_DEFAULT_LEVEL=debug` 写成 `LOG_DEFAULT_LEVEL_DEBUG = true`，并删掉同一 choice 的其他选项。choice 的选项不能单独设为 `false`。
- 没有提示（prompt）的符号拒绝写入。
- 用 toml_edit 修改，保留注释；覆盖已有值时保留行尾注释。
- 写完重新计算，设了却不生效的条目会给出原因（依赖没开、提示被隐藏、被 select、超出范围）。

```bash
idf config set FREERTOS_HZ=1000 LOG_DEFAULT_LEVEL=debug
idf -r config set COMPILER_OPTIMIZATION_PERF=y
```

#### idf config diff

```text
idf config diff [OPTIONS]
```

列出与 Kconfig 默认值不同的可设置符号，分成 `idf.toml` 里写了的和由它们连带变化的两组；choice 只列选中的选项。输出是可以直接粘进 `idf.toml` 的 TOML。与默认值相同的 `idf.toml` 条目单独列出。

#### idf config explain

```text
idf config explain [OPTIONS] <KEY>
```

解释一个符号：类型和范围、菜单路径、当前值及其来源（`idf.toml [config]`、profile、Kconfig 默认、被 select、由其他设置决定）、默认值、依赖及依赖符号的当前值、提示条件、select / imply 来源、带条件的默认值、定义所在的 Kconfig 文件和帮助文本。choice 列出全部选项，标出选中项和默认项。

#### idf config menu

```text
idf config menu [OPTIONS]
```

运行上游 menuconfig，退出后把改动写回 `idf.toml`。

- 需要终端（stdin 和 stdout 都是终端），否则报错 `menuconfig needs a terminal`。
- menuconfig 编辑的是按 `idf.toml` 生成的一份 sdkconfig，放在 `build/<target>/<profile>/menuconfig/`。构建自己的 sdkconfig 不受影响。
- 写回规则与 `set` 相同；改回 Kconfig 默认值的条目从表里删除（先验证删除后结果不变）。写回后再算一遍，与 menuconfig 的结果对不上时提示。

## 构建与设备

### idf build

构建固件。

```text
idf build [OPTIONS]
```

1. 确保 SDK 已安装（按 `idf.lock`；没有 lock 时解析并写入）。
2. 在 `build/<target>/<profile>/project/` 生成 CMake 工程。生成文件内容不变时不重写。
3. 生成文件有变化或还没有 `build.ninja` 时运行 CMake 配置。配置输出只在失败或 `-v` 时显示。
4. 有依赖时，把组件管理器的结果合并进 `idf.lock`。
5. 重新配置过时，检查 `[config]` 中未知的键、给 choice 赋值、没有提示的符号和没有生效的值，给出警告。
6. 运行 `cmake --build`，Ninja 的输出直接透传。
7. 在项目根目录放 `compile_commands.json` 符号链接，指向本次构建；那里已有真实文件时不动它。

完成时打印应用镜像的路径、大小和耗时。生成工程的结构见 [架构](/design/architecture)。

### idf run

构建、烧录，然后打开串口监视器。

```text
idf run [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `-b, --baud <BAUD>` | 烧录波特率。默认取用户配置 `flash_baud`，再默认 460800。 |

- 构建与 `idf build` 相同，烧录与 `idf flash` 相同，但烧录结束时不复位；监视器打开串口、设好波特率后复位一次，避免丢开头的输出。
- `-b` 只影响烧录。监视器的波特率取构建的 sdkconfig（见 [`monitor`](#idf-monitor)）。
- 芯片版本取烧录时连接读到的值，用于选择 ROM ELF。

### idf flash

构建并烧录。

```text
idf flash [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--app` | 只烧应用。 |
| `--bootloader` | 只烧 bootloader。 |
| `--partition-table` | 只烧分区表。 |
| `-b, --baud <BAUD>` | 烧录波特率。默认取用户配置 `flash_baud`，再默认 460800。 |

- 先构建，再按构建目录的 `flasher_args.json` 写入其中列出的所有镜像（含 otadata 等）。三个选择选项可以组合；都不给时写全部。构建里没有所选镜像时报错 `this build has no <role> image`。
- 镜像原样写入，只像 esptool 一样用 `0xFF` 补齐到 4 字节；板子上已经相同的镜像按 MD5 跳过。
- 写入前检查：镜像头里的芯片 ID 和芯片版本范围、镜像是否超出板子的 flash。检查失败时把芯片复位回原来的固件。构建配置的 flash 大小大于板子实际大小时只警告。
- 写完复位一次。

::: tip ESP32-P4 v3 以下
芯片版本不满足镜像要求时报错。ESP32-P4 v3 以下的板子需要在 `[config]` 里设置 `ESP32P4_SELECTS_REV_LESS_V3 = true`，报错信息里会提示。
:::

### idf monitor

串口监视器，带地址和栈回溯解码。

```text
idf monitor [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--no-reset` | 不复位芯片，直接接上正在运行的固件。 |
| `-b, --baud <BAUD>` | 波特率。 |

- 不构建，也不安装 SDK。不在项目里也能用，只是不解码。
- 波特率：`-b`，否则取当前 target/profile 构建的 sdkconfig 中的 `ESP_CONSOLE_UART_BAUDRATE`，再否则 `ESPTOOLPY_MONITOR_BAUD`，最后 115200。用户配置的 `flash_baud` 不影响监视器。
- 解码用的 ELF：应用 ELF（来自构建目录的 `project_description.json`）和 SDK 中 `esp-rom-elfs` 的 ROM ELF。ROM ELF 只在 `idf.lock` 锁定的 SDK 已安装时可用；单独运行时芯片版本取 sdkconfig 的 `ESP_REV_MIN_FULL`。构建里没有 ELF 时给出警告。
- 默认打开后复位芯片。
- 按键：Ctrl+R 复位；Ctrl+C 或 Ctrl+] 退出。其他按键发送给板子。
- 端口消失时打印 `<port> disconnected; waiting for it`，等同名端口回来后打印 `<port> is back` 并继续。
- stdout 不是终端或设置了 `NO_COLOR` 时去掉颜色序列。

### idf erase

擦除整片 flash、一个分区或一段区域。

```text
idf erase [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--partition <NAME>` | 按名字擦除一个分区，分区表从板子上读取。与 `--region` 互斥。 |
| `--region <OFFSET> <SIZE>` | 擦除 `OFFSET` 处的 `SIZE` 字节，必须是 4K 的非零倍数，例如 `0x9000 24K`。 |
| `-b, --baud <BAUD>` | 波特率。默认取用户配置 `flash_baud`，再默认 460800。 |

- 不带选项时擦除整片 flash。
- 不构建，不需要 SDK；在项目里时用项目的芯片检查连接的芯片。
- `OFFSET` 和 `SIZE` 的写法与 `idf.toml` 的分区大小相同：整数、`0x…`、`24K`、`1M`。对齐在连接芯片之前检查。
- `--partition` 从板子上读分区表，偏移取构建 sdkconfig 的 `PARTITION_TABLE_OFFSET`，默认 `0x8000`。板上没有有效分区表或没有该分区时报错，并列出板上的分区。
- 擦完复位进应用。

::: warning
擦除不可撤销。不带选项的 `idf erase` 会擦掉整片 flash，包括 NVS 中的数据。
:::

### idf image

导出合并后的固件镜像。

```text
idf image [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--format <FORMAT>` | `bin`（默认）、`uf2`、`dfu`。 |
| `-o, --output <OUTPUT>` | 输出文件。 |

- 先构建，再按 `flasher_args.json` 合并镜像。
- `bin`：等同 `esptool merge-bin`，各镜像放在各自偏移，空隙填 `0xFF`，从地址 0x0 开始烧。默认输出 `build/<target>/<profile>/merged-binary.bin`。
- `uf2`：等同 `esptool merge-bin --format uf2`。默认输出 `uf2.bin`。
- `dfu`：等同 `tools/mkdfu.py`，只支持 esp32s2、esp32s3、esp32p4。分块大小可由环境变量 `ESP_DFU_PART_SIZE` 指定。默认输出 `dfu.bin`。
- 三种格式的输出与 `idf native merge-bin / uf2 / dfu` 逐字节一致（在 ESP32-P4 构建上做过差分验证）。

### idf size

固件占用分析，调用 `esp-idf-size`。

```text
idf size [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `esp-idf-size` 的参数。 |

- 运行 `python -m esp_idf_size <ARGS>`。参数里没有以 `.map` 结尾的文件时，末尾补上构建目录里的 `<项目名>.map`。
- 不会先构建；没有 map 文件时报错并提示先运行 `idf build`。

```bash
idf size --format json2
```

### idf clean

清理构建产物。

```text
idf clean [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--all` | 删除所有 target 和 profile，即整个 `build/` 目录。 |

- 不带 `--all` 时删除当前 target 和 profile 的目录 `build/<target>/<profile>/`（受 `-t`、`-r`、`--profile` 影响）。
- 项目根目录的 `compile_commands.json` 符号链接指向被删除的构建时，一并删除。

`clean` 不碰全局缓存 `~/.idf-next/`；删除 SDK 用 [`idf sdk remove`](#idf-sdk-remove)。

## 调试与安全

这四个命令都是上游工具的薄封装：`idf` 补齐它已经知道的参数（芯片、串口、ELF 路径），在 SDK 的 Python 环境里运行上游工具，退出码原样传回。

::: tip 参数转交
命令名之后，从第一个 `idf` 不认识的参数开始，其余参数全部原样交给上游工具。紧跟在命令名后面的 `idf` 自己的选项（如 `--help`、`-p`）仍由 `idf` 解析；要把它们交给上游，先写 `--`，例如 `idf size -- --help`。
:::

### idf debug

启动 OpenOCD 和 GDB。

```text
idf debug [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `idf.py gdb` 的参数。 |

- 生成 CMake 工程后运行 `idf.py -C build/<t>/<p>/project -B build/<t>/<p> openocd gdb <ARGS>`。
- 没有设置环境变量 `OPENOCD_COMMANDS`，且选中的串口是芯片内置的 USB-Serial-JTAG（`303a:1001`），并且 SDK 的 OpenOCD 脚本里有 `board/<芯片>-builtin.cfg` 时，自动设置 `OPENOCD_COMMANDS=-f board/<芯片>-builtin.cfg`。这里的串口探测不会提示选择。
- 构建的 sdkconfig 里 `ESP32P4_SELECTS_REV_LESS_V3=y` 时，再追加 `-c 'gdb_memory_map disable'`：ESP32-P4 v3 以下 OpenOCD 无法读取 flash 映射，flash 里的断点改用硬件断点。
- 其他调试器（FTDI 或 UART 桥接板）沿用 SDK 默认的 OpenOCD 参数，需要自己设置 `OPENOCD_COMMANDS`。

### idf coredump

解析 core dump，调用 `esp-coredump`。

```text
idf coredump [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `esp-coredump` 的参数。 |

补齐规则（`--help` / `-h` 时只补芯片）：

- 没有 `--chip` 时补 `--chip <target>`；
- `ARGS` 非空，且没有 `--core`、`-c`、`--port`、`-p` 时，补选中的串口（从 flash 读取转储）；
- `ARGS` 非空，且没有以 `.elf` 结尾的参数时，末尾补应用 ELF；构建里没有 ELF 时报错。

`ARGS` 为空时只补 `--chip`。

::: warning
`coredump` 只验证过命令拼装，还没有用真实的转储测试过。
:::

### idf efuse

读取、汇总、烧写 eFuse，调用 `espefuse`。

```text
idf efuse [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `espefuse` 的参数。 |

- 没有 `--chip` / `-c` 时补 `--chip <target>`。
- 没有 `--virt`、`--port` / `-p`、`--help` / `-h` 时补 `--port <选中的串口>`。
- 真芯片（没有 `--virt`）上有两道额外保护，都在连接芯片之前生效：
  - 拒绝 `--do-not-confirm`；
  - 烧写类子命令必须在终端里手动输入 `burn` 才继续；stdin 或 stderr 不是终端时直接拒绝。
- `--virt` 虚拟模式下不加这两道保护，也不补端口。

烧写类子命令（需要输入 `burn`）：`burn-efuse`、`burn-block-data`、`burn-bit`、`burn-key`、`burn-key-digest`、`burn-custom-mac`、`set-flash-voltage`、`write-protect-efuse`、`read-protect-efuse`、`execute-scripts`。

只读子命令：`summary`、`dump`、`get-custom-mac`、`adc-info`、`check-error`。

```bash
idf efuse summary
idf efuse --virt burn-efuse ...
```

::: warning
eFuse 烧写不可逆，写错可能让芯片无法使用。上面两道保护之外，`espefuse` 自己的确认照常进行。这两份名单由 `tests/efuse_commands.rs` 对照已安装 `espefuse` 的 `--help` 检查。
:::

### idf secure

密钥、签名、flash 加密，调用 `espsecure`。

```text
idf secure [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `espsecure` 的参数。 |

不补任何参数，也不连接芯片，只是在 SDK 的 Python 环境里运行 `python -m espsecure <ARGS>`。

## 环境

### idf sdk

安装和选择 ESP-IDF SDK。一个 SDK 包括源码、工具链和 Python venv，安装在 `~/.idf-next/sdk/<version>/`。

```text
idf sdk [OPTIONS] <COMMAND>
```

#### idf sdk install

```text
idf sdk install [OPTIONS] [VERSION]
```

| 参数 | 说明 |
| --- | --- |
| `[VERSION]` | 例如 `6.0` 或 `6.0.1`（默认：项目要求的版本）。 |

- 带版本：解析匹配的最新正式发布（通过 GitHub API；`--offline` 时从已安装的 SDK 里找），安装它。不写 `idf.lock`。
- 不带版本：需要项目。按 `idf.lock` 精确安装；没有 lock 或 lock 与 `idf.toml` 不一致时，解析最新版本、安装并写 lock。
- 版本不在支持范围时报错，例如 `ESP-IDF 5.4 is not supported by idf-next 0.1.0; supported: >=6.0.0, <6.1.0`。
- 已经装好的部分不重复安装。安装期间持有 `~/.idf-next/.lock` 文件锁，另一个 `idf` 会等待。

```bash
idf sdk install 6.0
idf --offline --locked sdk install
```

#### idf sdk list

```text
idf sdk list [OPTIONS]
```

列出已安装的 SDK：版本、路径，以及括号里的说明（Python 版本、`incomplete`、`not supported by this idf-next`、`used by this project`）。

#### idf sdk use

```text
idf sdk use [OPTIONS] <VERSION>
```

把项目固定到一个 SDK 版本：修改 `idf.toml` 的 `idf`（保留格式和注释），然后在需要时安装并重新锁定。`VERSION` 可以是 `6.0` 或 `6.0.3`。`--locked` 下，如果要改 `idf.toml` 就报错。

#### idf sdk remove

```text
idf sdk remove [OPTIONS] <VERSION>
```

删除 `~/.idf-next/sdk/<VERSION>/`。`VERSION` 必须是精确版本（如 `6.0.3`）。下载缓存保留，所以之后可以 `--offline` 重装。当前项目的 `idf.lock` 用的就是这个版本时给出警告。

#### idf sdk path

```text
idf sdk path [OPTIONS] [VERSION]
```

打印 SDK 的 `IDF_PATH`（`~/.idf-next/sdk/<version>/esp-idf`）。不带版本时取项目 `idf.lock` 的版本；带版本时必须是精确版本。未安装时报错。

### idf env

显示 SDK 环境。

```text
idf env [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--shell` | 输出 POSIX shell 的 `export` 语句。 |

需要项目；SDK 未安装时先安装，没有 lock 时写入 lock。输出的变量见 [环境变量与用户配置](/reference/environment#idf-env)。

```bash
eval "$(idf env --shell)"
```

### idf doctor

诊断环境。只读，不安装任何东西。

```text
idf doctor [OPTIONS]
```

检查项：

- **Tool**：工具版本和支持的 IDF 范围、宿主平台、uv 的版本和路径、SDK 缓存位置。
- 在项目里时：
  - **Project**：`idf.toml` 能否解析；`idf.lock` 是否存在、是否与 `idf.toml` 一致；
  - **SDK**：lock 锁定的 SDK 是否已安装、是否与 lock 一致；每个工具按 tools.json 的 `version_cmd` 运行并按 `version_regex` 比对版本；
  - **Python**：venv 的解释器能否运行；运行 ESP-IDF 自己的 `idf_tools.py check-python-dependencies`。
- 不在项目里时：**Installed SDKs**，列出已安装的 SDK，标出不完整或超出支持范围的版本。

有失败项时以非零状态退出，并打印 `<n> problem(s) found`。

### idf native

在托管环境里运行上游 `idf.py`。

```text
idf native [OPTIONS] [ARGS]...
```

| 参数 | 说明 |
| --- | --- |
| `[ARGS]...` | 交给 `idf.py` 的参数。 |

- 先生成 CMake 工程，然后运行 `idf.py -C build/<t>/<p>/project -B build/<t>/<p> <ARGS>`，与 `idf build` 共用同一个构建目录和环境变量。
- 退出码原样透传。
- 用于 `idf` 还没覆盖的上游功能，例如 `idf native merge-bin`。

::: warning
`idf native … monitor` 被杀掉时，`idf.py` 的子进程不会跟着退出。交互时按 Ctrl+C 不受影响，因为信号发给整个进程组。
:::

### idf completions

生成 shell 补全脚本。

```text
idf completions [OPTIONS] <SHELL>
```

| 参数 | 说明 |
| --- | --- |
| `<SHELL>` | `bash`、`elvish`、`fish`、`powershell`、`zsh`。 |

```bash
idf completions zsh > ~/.zfunc/_idf
```
