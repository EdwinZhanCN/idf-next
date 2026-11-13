# idf-next 实施计划

> 本文件是 Claude Docs 文档《idf-next 实施计划》的本地转录（2026-11-13），
> 原文：https://claude.ai/artifact/Kuj2ej5ogY7MhwmDEL5BqA 。两边不一致时以原文为准。

## 目标与不变量

idf-next 是一个自用的 ESP-IDF 工具链：单个 Rust 二进制 `idf`，仓库里只留 `idf.toml`、`idf.lock` 和源码。高频的日常路径用 Rust 原生实现，其余委托给上游 Python 工具。Python 环境由 `idf` 托管，你看不到它。不兼容老项目，不追上游版本节奏。

不变量（任何实现决策都不能破坏）：

1. **项目目录**：只有 `idf.toml`、`idf.lock`、`src/`，以及可选的 `components/`。不出现 CMakeLists.txt、sdkconfig、idf_component.yml、dependencies.lock、managed_components/。
2. **外部依赖**：ESP-IDF SDK 源码、CMake、Ninja、交叉 GCC，以及该 SDK 版本对应的 Python venv。调试功能额外需要 OpenOCD 和 GDB。
3. **工具与 SDK 解耦**：idf-next 有独立的 semver 版本号，代码里声明支持的 IDF 版本范围。Python 依赖属于 SDK，跟着 SDK 版本走，不打包进工具。
4. **Python 不可见**：每个 SDK 版本的 venv 由工具附带的 uv 创建，和工具链一起装在 `~/.idf-next/sdk/<version>/` 下。不需要激活环境，也不需要 export.sh。
5. **原生与委托的分界**：高频、对延迟敏感、输出需要好看的功能原生实现；低频、高风险或体量大的功能委托上游。`idf native` 随时可以直通 idf.py。
6. **SDK 不打补丁**：IDF 的 CMake 框架和构建期脚本原样运行。
7. **单一版本锁定**：`idf.lock` 锁定 IDF commit、工具链哈希、Python 版本和依赖解析的哈希，与工具版本无关。升级是显式动作。
8. **产物隔离**：生成物全部放在 `build/`，SDK 及其工具链和 venv 放在全局缓存。
9. **正确性定义**：原生实现的部分，输出必须与被它替代的上游工具逐字节一致。

## CLI 草案复核

草案的骨架没问题，但有两处和不变量冲突，并且缺少约一半的功能面。

### 与不变量冲突

- **`native`（直通 idf.py）**：保留，作为逃生口。它在托管的 Python 环境里运行，并自动带上 `-B build/<target>`、`IDF_TARGET`、`SDKCONFIG` 等参数，因此和 `idf` 操作的是同一个工程。
- **全局 `--idf-version`**：每次调用都能临时换 SDK 版本，会让 `idf.lock` 形同虚设。版本只应该在两个地方指定：`new --idf <ver>` 和 `sdk use <ver>`。

### 需要明确语义

- **`-t/--target` 与 `target` 命令**：`-t` 是一次性覆盖，`target set` 写入 `idf.toml`。优先级为 CLI > `idf.toml`。构建目录按目标隔离（`build/<target>/`），否则切换芯片会触发全量重新配置。
- **串口参数**：`flash` 和 `monitor` 需要 `-p/--port`、`-b/--baud`。串口是机器相关的，不能写进 `idf.toml`，应该放在 `~/.config/idf/config.toml` 或环境变量 `IDF_PORT` 里，默认按 USB VID/PID 自动探测。
- **可复现性**：`--offline` 不够，还需要 `--locked`（lock 文件需要变更时直接报错）。
- **`create`**：拆成 `new <path>` 和 `init`（在当前目录初始化），沿用 Cargo 的语义。
- **`clean`**：加 `--all` 对应上游的 fullclean，同时清理全局缓存里属于该项目的条目。

### 缺失的命令

| 命令 | 对应上游 | 说明 |
| --- | --- | --- |
| `run` | build + flash + monitor | 最高频的命令，对应 `cargo run` |
| `size` | idf.py size / esp-idf-size | 按组件、文件、符号统计占用 |
| `erase` | erase-flash、erase-otadata | 支持整片擦除、按分区擦除、按地址范围擦除 |
| `image` | merge-bin、uf2、dfu | 导出各种格式的固件 |
| `debug` | openocd、gdb | 编排 OpenOCD 和 GDB，生成 gdbinit |
| `coredump` | esp-coredump | 解析 flash 或 UART 中的 coredump |
| `efuse` | espefuse | 读取、汇总、烧写 eFuse |
| `secure` | espsecure | 生成密钥、签名、加密和解密 |
| `update` | update-dependencies | 在约束范围内升级 lock |
| `tree` | 无 | 打印组件依赖树 |
| `completions` | 无 | 生成 shell 补全 |

### 修订后的命令树

```
Usage: idf [OPTIONS] <COMMAND>

Project:
  new <path>     Create a project in a new directory
  init           Create a project in the current directory
  add / remove   Manage dependencies
  update         Update idf.lock within constraints
  tree           Show dependency tree
  target         set | list
  config         menu | get | set | diff | explain

Build & device:
  build          Build firmware
  run            Build, flash, monitor
  flash          Flash firmware (--app, --bootloader, --partition-table)
  monitor        Serial monitor with backtrace decoding
  erase          Erase flash, partition, or region
  image          Export merged bin, UF2, DFU
  size           Size analysis
  clean          Clean build artifacts (--all)

Debug & security:
  debug          Launch OpenOCD + GDB
  coredump       Decode core dump
  efuse          Read / summary / burn eFuses
  secure         Keys, signing, flash encryption

Environment:
  sdk            install | list | use | remove | path
  env            Show environment (--shell to print exports)
  doctor         Diagnose environment
  native         Run upstream idf.py in the managed env
  completions    Generate shell completions

Options:
  -C, --project <PATH>
  -t, --target <CHIP>
  -p, --port <PORT>
  -r, --release
      --profile <NAME>
      --offline
      --locked
      --no-input
  -v, --verbose
  -q, --quiet
```

## 可复用的源码参考

每个模块分为原生、委托、按需 port 三类。原生的只有安装、烧录、监视、镜像、分区表和调试编排，其中大半已有现成的 Rust 实现。其余默认委托上游，等它们真正碍事时再 port。

| 模块 | 上游工具 | 已有 Rust 实现 | 策略 |
| --- | --- | --- | --- |
| 分区表 | `esp-idf/components/partition_table/gen_esp32part.py` | esp-idf-part 0.6.0 | 原生，直接依赖 |
| 烧录、镜像生成 | `espressif/esptool` | espflash 4.4.0，关闭 `cli` feature | 原生，直接依赖 |
| 串口监视 | `espressif/esp-idf-monitor` | espflash 自带 monitor 和回溯解码 | 原生，在 espflash 上扩展 |
| SDK、工具链安装 | `esp-idf/tools/idf_tools.py`、`tools/tools.json` | idf-im-lib | 原生，参考 EIM |
| UF2、DFU、merge-bin | `tools/mkuf2.py`、`tools/mkdfu.py` | 无 | 原生，体量很小 |
| 调试编排 | idf.py 的 openocd、gdb 子命令 | 无 | 原生，只负责启动进程和生成 gdbinit |
| Kconfig 求值 | `espressif/esp-idf-kconfig` | nom-kconfig（只做语法解析） | 委托 kconfgen；`config` 命令嫌慢时再 port |
| menuconfig | `espressif/esp-idf-kconfig` | 无 | 委托 |
| ldgen 等构建期脚本 | `esp-idf/tools/ldgen/` 等 | 无 | 委托，由 CMake 在托管环境里调用 |
| 组件管理 | `espressif/idf-component-manager`、Registry API 文档 | 无 | 先委托；需要全局缓存时再 port 求解器 |
| size | `espressif/esp-idf-size` | 无 | 委托，`idf` 负责补齐参数 |
| coredump | `espressif/esp-coredump` | 无 | 委托 |
| eFuse | `espressif/esptool`（espefuse） | 无 | 委托，不 port |
| 安全 | `espressif/esptool`（espsecure） | 无 | 委托，不 port |
| NVS、SPIFFS、FATFS 镜像 | 各自的生成脚本 | 无 | 委托 |
| 构建驱动 | `esp-idf/tools/idf_py_actions/` | `esp-rs/embuild`、`esp-idf-sys` | 参考它们如何绕过 idf.py 直接驱动 CMake |

组件管理的 manifest 不只是版本号，还支持条件依赖（按 target 或 IDF 版本）和 git、path 来源，这些都要照搬。registry 的协议以官方 API 文档为准，不要去逆向 Python 客户端。

## 可直接使用的 Rust 依赖

下表全部是成熟的通用 crate，ESP 专用的只有 espflash 和 esp-idf-part。表中的版本号只标注已经核实过的，其余在引入时锁定当时的最新稳定版。

| 领域 | crate | 用途 | 备注 |
| --- | --- | --- | --- |
| ESP 专用 | `espflash` 4.4.0 | 烧录、镜像生成、串口协议 | `default-features = false`，`cli` 模块不提供 SemVer 保证 |
| ESP 专用 | `esp-idf-part` 0.6.0 | 分区表 CSV 与二进制互转 | 支持 no_std，序列化需要 std |
| CLI | `clap`、`clap_complete` | 命令解析、shell 补全 | derive 模式 |
| CLI | `indicatif`、`miette` | 进度条、带源码位置的错误 | `idf.toml` 出错时能精确指出行列 |
| 配置 | `toml_edit` | 读写 `idf.toml` | `config set` 写回时保留注释和格式 |
| 配置 | `serde`、`serde_json` | 序列化 | |
| 配置 | YAML 解析库（待选） | 读取第三方组件的 `idf_component.yml` | `serde_yaml` 已停止维护，需要另选 |
| 依赖 | `pubgrub` | 版本求解 | |
| 依赖 | `semver` | 版本约束 | Cargo 语义，需核对与组件管理器的约束语法是否一致 |
| 网络 | `reqwest`（rustls）、`tokio` | registry API、工具链下载 | |
| 归档 | `tar`、`flate2`、`xz2`、`zip` | 解压工具链 | `tools.json` 里同时有这三种格式 |
| 文件 | `directories`、`fs4`、`walkdir`、`globset`、`which` | 缓存路径、并发锁、源码发现、工具定位 | 全局缓存必须加文件锁 |
| 解析 | `winnow` | 写 Kconfig 和 `.lf` 的解析器 | nom-kconfig 不够用时的方案 |
| 解析 | `regex`、`csv` | ldgen 匹配、各类 CSV 输入 | |
| ELF、DWARF | `object`、`gimli`、`addr2line` | 回溯解码、coredump、size | |
| TUI | `ratatui`、`crossterm` | menuconfig、monitor | |
| 加密 | `sha2`、`hmac`、`rsa`、`p256`、`p384`、`aes`、`xts-mode` | 签名、flash 加密、校验 | 初代 ESP32 的 flash 加密方案是定制的，要单独 port |
| 串口 | `serialport` | 端口枚举、按 VID/PID 探测 | espflash 已依赖 |
| 测试 | `insta` | 快照和差分测试 | |

分发方式：用 maturin 的 `bindings = "bin"` 把二进制打包成平台 wheel，通过 `uv tool install idf-next` 安装。wheel 唯一的 Python 依赖是 PyPI 上的 `uv` 包，它自带二进制，`idf sdk install` 直接调用同目录下的 uv 来创建 SDK venv。

## 架构

`idf` 是唯一的入口：它把 `idf.toml` 翻译成 `build/` 里的 CMake 工程，IDF 的 CMake 在构建时调用的 Python 脚本，运行在它托管的环境里。

```
项目目录 (idf.toml · idf.lock · src/) ──读取──▶ idf (Rust 单二进制: CLI · 配置求值 · 依赖求解) ──安装──▶ 全局缓存 (SDK · 工具链 · venv，按版本)
idf ──烧录·监视──▶ 设备 (flash · monitor · debug；espflash · OpenOCD + GDB)
idf ──生成──▶ build/ 生成物 (CMakeLists · sdkconfig.* · 链接脚本 · 固件镜像) ──▶ CMake + Ninja (IDF CMake 框架原样保留，调用 GCC 编译链接)
全局缓存 ──SDK 源码──▶ CMake + Ninja ──PYTHON=venv python──▶ 托管 Python 环境 (uv venv · 上游脚本原样运行 · kconfgen · ldgen · esptool 等) ──写回生成物──▶ build/
```

**托管 Python 环境**：配置 CMake 时传入 `-DPYTHON=<SDK venv 里的 python>`，上游的构建期脚本原样运行。venv 属于 SDK：`idf sdk install` 调用工具附带的 uv，按该 IDF 版本的 requirements 和 constraints 创建。uv 的缓存用硬链接复用包文件，多个 SDK 版本并存几乎不占额外空间。将来要原生替换某个脚本时，只需把 `PYTHON` 换成一个分发 shim，未替换的调用继续转发给 venv。

**全局缓存与版本**：`~/.idf-next/sdk/<version>/` 存放一个 SDK 的全部内容（源码、工具链、venv），一起安装、一起删除；第三方组件放在 `~/.idf-next/registry/`。所有内容按版本和哈希只读存放，写入时加文件锁。工具本身由 `uv tool install` 和 `uv tool upgrade` 管理，和 SDK 互不影响：升级工具不动 SDK，切换 SDK 不需要重装工具。项目只通过 `idf.lock` 引用具体的 SDK 版本。

**文件模型**：`idf.toml` 是唯一的事实来源，示例如下。分区表采用数组形式，以保证分区顺序。

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"

[config]
FREERTOS_HZ = 1000

[dependencies]
"espressif/led_strip" = "*"

[[partition]]
name = "nvs"
type = "data"
subtype = "nvs"
size = "24K"

[[partition]]
name = "factory"
type = "app"
subtype = "factory"
size = "1M"

[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true
```

## 分阶段实施计划

共 6 个阶段，P2 结束后就能日常使用。P5 之后不再按计划推进，只在委托的工具真正碍事时才 port。目标版本锁定 ESP-IDF v6.0.x。

1. **P0 骨架与分发**
   - 定义 `idf.toml` 和 `idf.lock` 的 schema，搭好 CLI 骨架。
   - 配置 maturin 打包，声明 `uv` 依赖，发布到 macOS arm64、Linux aarch64、Linux x86_64 三个平台。
   - 在代码里声明支持的 IDF 版本范围。
   - **门槛**：`uv tool install idf-next` 后，`idf --version` 能正确报告工具版本和支持的 IDF 范围。
2. **P1 SDK 与工具链**
   - 原生实现 `sdk install | use | list | remove | path`，一次装齐源码、工具链和 venv。
   - `env` 和 `doctor`，`doctor` 检查 SDK 版本是否在工具支持的范围内。
   - **门槛**：删掉缓存后，能根据 lock 文件离线复现同一套环境，包括 venv。
3. **P2 构建与设备闭环**
   - `build`：在 `build/` 里生成 CMake 工程，用 SDK 的 venv 调用 CMake 和 Ninja；`native` 直通 idf.py。
   - 把 `compile_commands.json` 链接到项目根目录。
   - 原生实现 `flash`、`monitor`（回溯解码）、`run`、`erase`、`image`。
   - **门槛**：IDF 自带的 examples 能构建，项目目录里没有多余文件，在你自己的开发板上可以日常使用。
4. **P3 配置**
   - `[config]` 生成 sdkconfig.defaults。
   - `config get | set | diff | explain` 读取 kconfgen 输出的 JSON。
   - menuconfig 委托上游，退出后把差异写回 `idf.toml`。
   - **门槛**：改配置只需要动 `idf.toml`。
5. **P4 依赖**
   - `add | remove | update | tree` 维护 `idf.toml`，在 `build/` 里生成 manifest 交给组件管理器，dependencies.lock 的内容合并进 `idf.lock`。
   - **门槛**：项目目录里不出现 managed_components 和 dependencies.lock。能否做到取决于组件管理器的路径配置，需要验证；做不到就 port 求解器。
6. **P5 长尾薄封装**
   - `size`、`debug`、`coredump`、`efuse`、`secure` 由 `idf` 自动补齐芯片、串口、ELF 路径等参数，再调用上游工具。
   - **门槛**：不需要手写任何上游命令的参数。

之后按需 port：某个委托的工具慢、输出难看或者经常出问题，才把它搬过来，并用差分测试保证一致。最可能的候选是 Kconfig 求值（让 `config` 命令不必等 CMake 配置）和依赖求解器（让第三方组件进入全局缓存）。

## 风险与开放问题

改成委托上游之后，主要风险从“port 是否正确”变成了两件事：托管环境能否复现，上游写进项目目录的文件能否藏进 `build/`。

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| SDK venv 无法复现 | 上游的 pip 依赖漂移 | 用工具附带的 uv 严格按该版本的 constraints 安装，解析结果的哈希写进 `idf.lock` |
| 工具与 SDK 版本不兼容 | 新工具生成的工程不适用于旧 SDK | 代码里声明支持的 IDF 范围，`doctor` 检查，超出范围直接报错 |
| `native` 的环境检查 | idf.py 启动时校验 Python 环境，可能拒绝运行 | 用环境变量把它指向 SDK venv，具体变量名对照 v6.0 源码确认 |
| managed_components 的位置 | 可能无法移出项目目录 | P4 验证；不行就 port 求解器，由 `idf` 自己填充组件 |
| 解析上游工具的输出 | 上游一改格式就会坏 | 只负责传参和透传输出，不解析给人看的文本；需要结构化数据时用它们的 JSON 输出 |
| v6.0 的 Build System v2 目前是预览版 | 后续版本可能改变 CMake 入口 | 先基于 v1，等 v2 稳定后再评估 |
| eFuse 烧写不可逆 | 硬件永久损坏 | 委托 espefuse，`idf` 只额外加一道二次确认 |

开放问题：

- 用户自定义组件的配置项写在 `idf.toml` 里，由工具翻译成 Kconfig，还是允许组件保留自己的 Kconfig 文件？
- 默认支持哪些芯片？测试矩阵的规模取决于这个答案。

---

## 实施记录

### P0（2026-11-13）

- 支持范围：`src/idf_support.rs` 中 `SUPPORTED_IDF_RANGE = ">=6.0.0, <6.1.0"`；`idf --version` 输出工具版本与该范围，`-V` 只输出单行版本。
- `idf.toml` schema（`src/manifest.rs`）：
  - `project.idf` 只接受发行系列 `"6.0"` 或确切版本 `"6.0.1"`，不接受任意 semver 约束（单一版本锁定）；超出支持范围直接报错。
  - `[config]` 键不带 `CONFIG_` 前缀，值为布尔、整数或字符串；`[profile.<name>.config]` 叠加在其上，`dev`、`release` 为内置 profile。
  - `[dependencies]` 值为约束字符串，或含 `version` / `git`（可选 `rev`、`subdir`）/ `path` 之一的表；条件依赖用 `targets = [...]` 和 `idf = "6.0"`。版本约束字符串原样保存，语法核对留到 P4。
  - `[[partition]]`：`name`（≤15 ASCII 字符，唯一）、`type`、`subtype`、`size`、可选 `offset`、`encrypted`、`readonly`；大小写法支持整数、`"0x…"`、`"24K"`、`"1M"`。
  - 所有错误经 miette 指向出错的行列。
- `idf.lock` schema（`src/lockfile.rs`）：`version = 1`、`[idf] version/commit`、`[python] version/requirements`（`sha256:` 摘要）、`[[tool]]`（每个主机平台一条 archive：url/sha256/size）、`[[component]]`（source 用 `registry+URL` / `git+URL#COMMIT` / `path+PATH`）。序列化确定性排序，原子写入。
- 芯片名（`src/chip.rs`）：接受任何 `esp32*` 形式的名字，`KNOWN_CHIPS` 列出 v6.0 的非预览芯片；默认测试哪些芯片仍是开放问题。
- CLI（`src/cli.rs`）：完整的修订命令树已可解析，未实现的命令报错并注明所属阶段。`completions` 已可用。`-b/--baud` 放在 `run`/`flash`/`monitor` 上，`-p/--port` 可由 `IDF_PORT` 提供。
- 分发：`pyproject.toml`（maturin `bindings = "bin"`，依赖 `uv`）；`.github/workflows/release.yml` 构建三个平台的 wheel，打 `v*` tag 时发布到 PyPI。本机已验证 macOS arm64 wheel 经 `uv tool install` 后 `idf --version` 正确，`uv` 与 `idf` 装在同一 bin 目录。

### P1（2026-11-13）

门槛已验证（macOS arm64，ESP-IDF v6.0.3）：`idf sdk remove 6.0.3` 后执行 `idf --offline --locked sdk install`，重建出的 SDK（源码、工具链、venv 共 50,559 个文件）与删除前逐字节一致，`idf.lock` 不变。

- **全局缓存布局**（`src/home.rs`）：`sdk/<version>/` 下放 `esp-idf/`、`tools/<name>/<version>/`、`python/<摘要前16位>/`；`downloads/` 是按哈希寻址的下载存储，`python/` 放 uv 管理的解释器，`uv-cache/` 是 uv 的 wheel 缓存。`idf sdk remove` 只删 `sdk/<version>/`，离线重装依赖其余三者。根目录可用 `IDF_NEXT_HOME` 覆盖。所有写入持有 `~/.idf-next/.lock` 文件锁。
- **IDF 源码**（`src/idf_source.rs`）：官方 release zip 约 2 GB，其中 1.86 GB 是 `.git`。先读 zip 中央目录，只按 HTTP Range 下载非 `.git` 条目（约 130 MB）到稀疏文件，再解压。每个条目校验 CRC-32，整棵源码树计算摘要写入 `idf.lock` 的 `[idf] source`。没有 `.git`，所以写入 `version.txt`（IDF 在无 git 时本来就读它，不算打补丁）。版本 x.y.0 的 tag 是 `vX.Y` 而不是 `vX.Y.0`。
- **工具链**（`src/tools_json.rs`）：读取 SDK 自带的 `tools/tools.json`（格式 v3），对宿主平台应用 `platform_overrides`，安装所有 `install: always` 的工具，另外加装 `cmake` 和 `ninja`，不依赖系统里的 CMake/Ninja。归档按 sha256 和大小校验，4 线程并行下载解压。`idf.lock` 记录三个支持平台的全部归档，换机器也能用同一份 lock。
- **Python 环境**（`src/python.rs`）：用 wheel 里附带的 uv（通过解析 `idf` 的真实路径找到同目录的 uv）执行 `uv pip compile --universal --generate-hashes --no-annotate --exclude-newer <时间>`，输入是 SDK 的非可选 requirements 和 Espressif 的 constraints 文件。`idf.lock` 的 `[python]` 记录解释器精确版本、`exclude-newer`、constraints 摘要和解析结果摘要。解析结果和 constraints 文件按摘要存进下载存储；缺失时用同样的输入重新解析，结果摘要必须与 lock 一致。venv 目录以解析结果摘要命名，同一 SDK 版本下不同项目的解析不会互相覆盖。不使用 Espressif 的额外 pip 索引，只用 PyPI。默认解释器 3.12。
- **与上游的接口**：`IDF_TOOLS_PATH` 指向 venv 所在目录（里面放着 `espidf.constraints.v6.0.txt`），`IDF_PYTHON_ENV_PATH` 指向 venv。这样 IDF 自己的 `idf_tools.py check-python-dependencies`（CMake 每次构建前都会跑）原样通过，`idf doctor` 也直接调用它。
- **命令**：`sdk install [版本]`（项目内按 lock 安装，没有 lock 就解析最新版本并写 lock）、`sdk use <版本>`（用 toml_edit 改 `idf.toml`，保留格式和注释，然后重新锁定）、`sdk list`、`sdk remove <精确版本>`、`sdk path [版本]`（打印 IDF_PATH）、`env [--shell]`（需要时会自动安装）、`doctor`（只读：检查 uv、lock 与 `idf.toml` 是否一致、每个工具执行 `version_cmd` 并按 `version_regex` 精确比对版本、IDF 的 Python 依赖检查）。`--offline` 禁止任何网络访问；`--locked` 在 lock 缺失或需要变更时直接报错。
- **镜像**：支持 `IDF_GITHUB_ASSETS`，语义与 idf_tools.py 一致；GitHub API 支持 `GITHUB_TOKEN`。

留给 P2 的观察：

- 在托管环境里运行 `idf.py --version` 能输出 `ESP-IDF v6.0.3`，但会先打印 "not a git repository" 警告，因为 SDK 里没有 `.git`。CMake 的 `git_submodule_check` 在没有 `.git` 时同理，构建时可设置 `IDF_SKIP_CHECK_SUBMODULES=1`。
- 运行上游 Python 脚本会在共享的 SDK 目录里写 `__pycache__`，不影响正确性，但 SDK 目录在使用后不再只读。
