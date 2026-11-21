---
title: 路线图与现状
description: idf-next 各阶段（P0–P5）完成了什么、门槛如何验证、在什么硬件上验证，以及已知限制与待办。
---

# 路线图与现状

idf-next 按 6 个阶段实施，目标版本锁定 ESP-IDF v6.0.x。P0 到 P5 已全部完成。P5 之后不再按计划推进，只在某个委托的工具真正碍事时才 port（见 [设计原则](/design/principles#when-to-port)）。

本页内容取自 `docs/PLAN.md` 的「实施记录」。

## 总览

| 阶段 | 内容 | 完成日期 | 门槛 |
| --- | --- | --- | --- |
| P0 | 骨架与分发 | 2026-11-13 | `uv tool install idf-next` 后，`idf --version` 报告工具版本和支持的 IDF 范围 |
| P1 | SDK 与工具链 | 2026-11-13 | 删掉缓存后，能按 lock 离线复现同一套环境，包括 venv |
| P2a | 构建闭环 | 2026-11-13 | IDF 自带的示例能构建，项目目录里没有多余文件 |
| P2b | 设备闭环 | 2026-11-19 | 在开发板上日常可用 |
| P3 | 配置 | 2026-11-20 | 改配置只需要动 `idf.toml` |
| P4 | 依赖 | 2026-11-20 | 项目目录里不出现 `managed_components` 和 `dependencies.lock` |
| P5 | 长尾薄封装 | 2026-11-20 | 不需要手写上游命令里 `idf` 已经知道的参数 |

## 验证环境

- **主机**：macOS arm64（M 系列 Mac）。发布流程为 macOS arm64、Linux x86_64、Linux aarch64 三个平台构建 wheel；本机验证过的是 macOS arm64。
- **SDK**：ESP-IDF v6.0.3。
- **开发板**：ESP32-P4，芯片版本 v1.3，USB-Serial-JTAG，16 MB flash。这是唯一上板验证过的设备。

::: warning 硬件覆盖有限
所有设备相关的验证都在这一块 ESP32-P4（USB-Serial-JTAG）上完成。UART 桥接芯片（CP210x、CH34x）的板子、FTDI 调试器，以及 P4 以外的真芯片上的 `flash`、`monitor`、`debug`、`efuse` 都没有实测。
:::

## P0 骨架与分发

完成：

- 支持范围写在 `src/idf_support.rs`：`SUPPORTED_IDF_RANGE = ">=6.0.0, <6.1.0"`。`idf --version` 输出工具版本和该范围，`-V` 只输出一行版本。
- `idf.toml` 和 `idf.lock` 的 schema（`src/manifest.rs`、`src/lockfile.rs`），所有错误经 miette 指出行列。
- 完整的命令树已可解析，`completions` 可用。
- 分发：`pyproject.toml` 用 maturin 的 `bindings = "bin"` 打包，唯一的 Python 依赖是 `uv`；`.github/workflows/release.yml` 构建三个平台的 wheel，打 `v*` tag 时发布到 PyPI。

门槛验证：本机 macOS arm64 的 wheel 经 `uv tool install` 安装后，`idf --version` 输出正确，`uv` 与 `idf` 装在同一个 bin 目录。CI 在 macOS arm64 和 Linux x86_64 上安装 wheel 并检查 `idf --version`；Linux aarch64 的 wheel 是交叉编译的，CI 里无法运行。

## P1 SDK 与工具链

完成：

- 全局缓存布局（`~/.idf-next/`）、文件锁、`IDF_NEXT_HOME`。
- IDF 源码：只按 HTTP Range 下载 release zip 中非 `.git` 的条目（约 130 MB，整个 zip 约 2 GB），逐条校验 CRC-32，源码树摘要写进 `idf.lock`。
- 工具链：读 SDK 的 `tools.json`，安装 `install: always` 的工具加 `cmake` 和 `ninja`，4 线程并行下载解压；lock 记录三个平台的全部归档。
- Python：用附带的 uv 按固定的 `--exclude-newer` 解析，venv 以解析结果摘要命名，只用 PyPI，默认解释器 3.12。
- 命令：`sdk install | use | list | remove | path`、`env [--shell]`、`doctor`；`--offline`、`--locked`。
- 支持 `IDF_GITHUB_ASSETS` 镜像和 `GITHUB_TOKEN`。

门槛验证（macOS arm64，ESP-IDF v6.0.3）：`idf sdk remove 6.0.3` 后执行 `idf --offline --locked sdk install`，重建出的 SDK（源码、工具链、venv 共 50,559 个文件）与删除前逐字节一致，`idf.lock` 不变。

## P2a 构建闭环

完成：`new`、`init`、`target`、`build`、`clean`、`native`；在 `build/<target>/<profile>/` 生成 CMake 工程；增量构建；`compile_commands.json` 链接到项目根目录。

门槛验证：先统计 ESP-IDF v6.0.3 自带的 458 个示例：

- 192 个依赖组件管理器，属于 P4；
- 63 个在 CMake 里有自定义逻辑（`EMBED_FILES`、按 target 写的分支、cmakev2 等）；
- 其余 195 个可以直接转成 `idf.toml` 布局。

用转换脚本把这 195 个转成 `idf.toml` 布局，按类别抽样 33 个，用 `idf --offline --locked build` 构建，覆盖 esp32、esp32s3、esp32c6、esp32h2、esp32p4，全部通过。

- 第一轮暴露了一个问题：`Kconfig.projbuild` 里的 `rsource` 按相对路径找文件，复制到生成目录后就找不到了。现在改为在生成目录放一个 `source "<绝对路径>"` 的存根。
- `security/nvs_encryption_hmac` 需要在 `[config]` 里写 `NVS_SEC_HMAC_EFUSE_KEY_ID = 0`，上游示例本身也要求手动设置这个值。
- 所有示例构建后，项目目录里都只有 `idf.toml`、`idf.lock`、`src/`、`components/` 和 `compile_commands.json` 链接。

完整构建一个示例约 25 秒，不改代码再次构建约 0.1 秒（M 系列 Mac）。

## P2b 设备闭环

完成：`flash`、`monitor`（回溯解码）、`run`、`erase`、`image`；串口按 VID/PID 自动探测；espflash 4.7.0（锁定版本，开启 `cli` feature 只为复用 `ResolvingPrinter`，`rust-version` 随之提到 1.95）。

门槛验证：在 ESP32-P4 v1.3（USB-Serial-JTAG，16 MB flash，macOS arm64）上，`run`、`flash`、`monitor`、`erase`、`image` 的日常用法全部可用。

- P4 panic 时监视器能把栈转储还原成完整调用链，带源码行号。
- `image` 的三种格式与 `idf native merge-bin / uf2 / dfu` 的输出逐字节一致；DFU 在 `ESP_DFU_PART_SIZE=0x10000` 分块时也一致。
- 复位时序：烧录结束只由 espflash 复位一次，`run` 改为由监视器打开端口后复位，避免 P4 上的二次复位（见 [架构](/design/architecture#run-flow)）。

后续补充（2026-11-20）：

- **拔插重连**已在 P4 上实测：`idf run` 之后拔掉 USB，监视器打印 `disconnected; waiting for it`，插回后打印 `is back` 并在同名端口自动接上，不需要手动操作。端口重新出现时 ROM 和 bootloader 的开头几行已经输出完，所以重连后的输出从 `cpu_start` 附近开始。
- **用户配置** `~/.config/idf/config.toml`（`port`、`flash_baud`）：端口和优先级已验证；`flash_baud` 上板确认，配置为 115200 时没有 espflash 的高波特率警告，默认 460800 和 `--baud` 覆盖时都有。

## P3 配置

完成：`[config]` 生成 `sdkconfig.defaults`；`config get | set | diff | explain | menu`；构建时检查 `[config]`；只有 defaults 或分区表变化时 `config` 命令不跑 CMake 配置。

门槛验证：在 hello 项目上用 `config set` 和 `config menu` 改了分区表偏移（0x9000）、flash 大小（16 MB）、tick rate、日志级别和 release 的优化选项，构建后烧到 P4 板子上。启动日志里分区表在 0x9000、flash 为 16 MB，应用正常运行；项目目录里仍只有 `idf.toml`、`idf.lock` 和 `src/`。

## P4 依赖

完成：`add | remove | update | tree`；`[dependencies]` 翻译成生成的 `idf_component.yml`；组件管理器的结果合并进 `idf.lock`；全新构建目录按 lock 预置；组件下载缓存在 `~/.idf-next/components/` 共用。

门槛验证：项目目录里没有 `managed_components` 和 `dependencies.lock`，也不需要 port 求解器，因为组件管理器把生成的 `build/<target>/<profile>/project/` 当作项目目录。在 hello 项目上用注册表（`espressif/led_strip`、`espressif/button`）、git（`idf-extra-components` 的 `pid_ctrl` 子目录）和本地路径三种依赖构建，烧到 P4 板子上，应用调用了路径依赖里的函数并正常输出。

## P5 长尾薄封装

完成：`size`、`secure`、`coredump`、`efuse`、`debug`，都是 `python -m <工具>`（`debug` 是 `idf.py openocd gdb`）加上补齐的参数，退出码原样传回。

门槛验证：

| 命令 | 验证 |
| --- | --- |
| `size` | 与 `idf native size` 逐字对比，`--format json2` 内容一致 |
| `secure` | 离线生成密钥、签名、验证，结果与直接调用 `espsecure` 相同 |
| `coredump` | 只验证了命令拼装，没有解码过真实的转储 |
| `efuse` | `--virt` 下 `summary` 与直接调用逐字一致，虚拟烧写可用；真芯片上只在 P4 上跑过只读的 `summary`，没有烧过任何 eFuse |
| `debug` | 在 P4 v1.3 的内置 USB-Serial-JTAG 上，自动使用 `board/esp32p4-builtin.cfg`；加 `gdb_memory_map disable` 后 GDB 可以连接，`app_main` 的断点用硬件断点 |

eFuse 门禁的烧写命令名单和只读命令名单是手写的。`tests/efuse_commands.rs` 读取已安装 `espefuse` 对 12 个芯片的 `--help` 中的 “Burn commands” 和 “Read commands”，要求前者全在烧写名单里、后者全在只读名单里，出现没见过的分组也失败（临时删掉一项确认过测试会失败）。没装 SDK 时跳过。

这五个命令没有移植乐鑫的测试：它们只是调用上游，上游测试测的是上游内部逻辑。

## 已知限制与待办

### 设备

- `monitor` 没有日志落盘和时间戳，也没有 esp-idf-monitor 的 Ctrl+T 菜单。
- 只在 USB-Serial-JTAG 上验证过；UART 桥接（CP210x、CH34x）的复位时序沿用 espflash 的，没有实测（缺带桥接芯片的设备）。
- `idf native … monitor` 被杀掉时，`idf.py` 的子进程不会跟着退出（交互时 Ctrl+C 发给整个进程组，不受影响）。

### 构建

- `main` 组件的 `EMBED_FILES` / `EMBED_TXTFILES` 和自定义 CMake 逻辑无法在 `idf.toml` 里表达。
- 在托管环境里运行 `idf.py` 时会先打印 “not a git repository” 警告，因为 SDK 里没有 `.git`。
- 运行上游 Python 脚本会在共享的 SDK 目录里写 `__pycache__`，SDK 目录在使用后不再只读。
- bootloader 子工程在 sdkconfig 变化后会自己重新跑一次 CMake，它的输出会漏到 `idf build` 的输出里（`idf.py` 也是这样）。

### 配置

- `config set` 只检查要设的值本身；打开一个选项连带改变的其他值，目前只能在 `config diff` 里看到。
- 提示被隐藏的符号（例如依赖 `ESP_CONSOLE_UART_CUSTOM` 的 `ESP_CONSOLE_UART_BAUDRATE`），`set` 照样写入，只给警告，因为用户可能正准备同时打开它的前提。

### 依赖

- 全新构建目录第一次配置仍要联网：没有 `manifest_hash` 就得求解，组件管理器要读注册表的元数据。真要离线复现，要么 port 求解器，要么自己计算 `manifest_hash`。
- `add`、`update`、`tree` 求解时给组件管理器的是上次配置留下的 `config/sdkconfig.json`；还没配置过就没有，依赖 manifest 里按 Kconfig 取值的 `matches` 规则不生效。配置之后改了 `[config]`，这个文件在下次构建前是旧的。
- 有 `targets` 限制的依赖时，`idf.lock` 是各芯片的并集，换芯片构建只增补条目，不会清掉别的芯片的条目。
- 组件管理器的 git 缓存对 `file://` 本地仓库有问题（新提交报 `not a tree object`），真实的 https 仓库没有问题；没有深究。

### 调试与安全

- `debug` 只在 USB-Serial-JTAG 上试过；FTDI 或桥接的板子仍用 SDK 默认的 OpenOCD 参数，需要用 `OPENOCD_COMMANDS` 自己指定。`gdb_memory_map disable` 的处理只在 P4 v1.3 上验证过。
- `coredump` 没有用真实转储测过。
- `efuse` 只在 P4 上跑过真芯片的只读 `summary`，其他芯片只有 `--virt` 覆盖；`secure` 不碰芯片。

### 平台与分发

- 只支持 macOS arm64、Linux x86_64、Linux aarch64。其他主机（包括 Windows）上 `idf` 报错 `unsupported host`。
- 终端 logo 使用 Espressif 标志，属于商标，按 press kit 的品牌指南使用；wheel 发布到 PyPI 前需要确认这一点。

## 风险与开放问题

| 风险 | 状态 |
| --- | --- |
| SDK venv 无法复现 | 已应对，P1 门槛验证 |
| 工具与 SDK 版本不兼容 | 已应对：支持范围写在代码里，`doctor` 检查，超出范围报错 |
| `native` 的环境检查 | 已解决：`IDF_TOOLS_PATH` 和 `IDF_PYTHON_ENV_PATH` 指向托管 venv |
| `managed_components` 的位置 | 已解决，P4 门槛验证 |
| 解析上游工具的输出 | 持续遵守：只透传，结构化数据用 JSON |
| v6.0 的 Build System v2 是预览版 | 仍基于 v1，等 v2 稳定后再评估 |
| eFuse 烧写不可逆 | 委托 `espefuse`，加二次确认；真芯片上未做过烧写 |

开放问题：

- 用户自定义组件的配置项写在 `idf.toml` 里由工具翻译成 Kconfig，还是允许组件保留自己的 Kconfig 文件？
- 默认支持哪些芯片？测试矩阵的规模取决于这个答案。

## 之后

只在委托的工具慢、输出难看或者经常出问题时才把它搬过来，并用差分测试保证一致。最可能的候选：

- **Kconfig 求值**：让 `config` 命令不必等 CMake 配置。
- **依赖求解器**：让第三方组件进入全局缓存，也让全新构建目录可以离线配置。
