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

### 插曲：终端 logo（2026-11-13，不属于任何阶段）

需求来自 Espressif press kit 那边的会话，规格见 press kit 的 `cli-logo-handoff/README.md`。做法参照 Apple `fm` CLI：用 Unicode 盲文字符（每格 2×4 个点）画出 Espressif 标志，配 24 位色的对角渐变 #FF8246 → #E8362D → #B8123C。

- **不写死字符格比例**：二进制里嵌入 512×512 的 1-bit 掩码（`assets/espressif_symbol_512.pbm`，32 KB），运行时按终端真实的格高与格宽之比栅格化，所以 logo 在任何字体下都是正方形。比例依次取自 `TIOCGWINSZ` 的像素尺寸、`CSI 16 t` 查询（需要 stdin 和 stdout 都是终端，超时 80 ms），都拿不到就用 2.0，最后限制在 1.6–2.6。代码在 `src/logo.rs`，从 press kit 的 `reference_render.py` 移植而来；快照测试与 Python 参考实现的输出逐字节一致（盲文、ASCII 和 truecolor 三种模式）。
- **什么时候画**：只在 `idf`（不带参数）、`idf --help`、`idf help` 和 `idf --version` 时画，子命令的 help 不画。stdout 必须是终端，`TERM=dumb` 和 `-q` 时不画。管道和脚本拿到的输出与以前完全相同，见 `src/banner.rs`。
- **位置**：logo 在左，右侧垂直居中放版本号、简介和支持的 IDF 范围；help 正文接在下面，开头的简介行移到 logo 右侧，不重复。尺寸从 31、27、20 列里选能放下的最大一档；help 的 logo 不超过屏幕高度的一半，version 的 logo 不超过整屏。放不下就不画。
- **颜色与字符集**：设置了 `NO_COLOR` 时不上色；`COLORTERM=truecolor/24bit` 时用 24 位色；`TERM` 为 linux、ansi 或 vt* 时只用红色；其余用 256 色。locale 不是 UTF-8 时退回 ASCII 字符。
- Espressif 标志是商标，按 press kit 的品牌指南使用。wheel 发布到 PyPI 前需要确认这一点。

### P2a 构建闭环（2026-11-13）

P2 分成两步：P2a 是不需要硬件的构建闭环（`new`、`init`、`target`、`build`、`clean`、`native`），P2b 是设备闭环（`flash`、`monitor`、`run`、`erase`、`image`），后者要在开发板上验证。

- **生成的工程**（`src/build.rs`）：每个目标和 profile 一个目录，`build/<target>/<profile>/`，它本身就是 CMake 的二进制目录。里面的 `project/` 是生成的 CMake 源目录：
  - `CMakeLists.txt`：设置 `SDKCONFIG`、`SDKCONFIG_DEFAULTS` 和 `EXTRA_COMPONENT_DIRS`（指向项目的 `components/`），然后原样 include IDF 的 `project.cmake`。
  - `main/CMakeLists.txt`：用绝对路径列出 `src/` 下的所有源文件（.c/.cc/.cpp/.cxx/.S/.s，递归，跳过隐藏目录），`INCLUDE_DIRS` 指向 `src/`。`src/Kconfig` 和 `src/Kconfig.projbuild` 会复制到这里，因为 IDF 只在组件目录里找它们。
  - `sdkconfig.defaults`：由 `[config]` 加上当前 profile 的配置生成。值形如 `"0x…"` 的字符串原样输出，因为 Kconfig 会把 hex 类型的值一律按十六进制读，`4096` 会被当成 0x4096。
  - `partitions.csv`：来自 `[[partition]]`，同时自动加上 `PARTITION_TABLE_CUSTOM`。
  - 所有路径都用 CMake 的方括号参数写出，不用转义。
- **增量**：生成的文件内容不变就不重写。`sdkconfig.defaults` 一旦变化就删掉 `sdkconfig`，因为 IDF 的 defaults 只填补 sdkconfig 中缺失的值，而 `idf.toml` 才是唯一的事实来源。任何生成文件变了，或者还没有 `build.ninja`，就重新执行 CMake 配置；其余情况交给 Ninja 自己判断。不改代码再次构建约 0.1 秒。
- **环境**：使用 P1 的 SDK 环境，另外设置 `IDF_TARGET`、`IDF_SKIP_CHECK_SUBMODULES=1`（SDK 里没有 `.git`）和 `IDF_COMPONENT_MANAGER=0`（依赖由 P4 接管）。`-DPYTHON` 指向 venv。CMake 配置阶段的输出被收起，只在失败或 `-v` 时显示；Ninja 的输出直接透传。
- **`compile_commands.json`**：项目根目录放一个符号链接，指向最近一次构建的结果。如果那里已经有一个真实文件，就不去动它。
- **`native`**：执行 `idf.py -C build/<t>/<p>/project -B build/<t>/<p>`，与 `idf build` 共用同一个构建目录，退出码原样透传。
- **`clean`**：删除当前目标和 profile 的目录；`--all` 删除整个 `build/`，并移除失效的 `compile_commands.json` 链接。
- **`new` / `init`**：生成 `idf.toml`、`src/main.c` 和 `.gitignore`（忽略 `/build/` 和 `/compile_commands.json`）。默认芯片是 esp32，可用 `-t` 指定。`init` 不会覆盖已有的源码。
- **`target set`**：用 toml_edit 修改 `idf.toml`，保留注释和格式。
- 暂不支持：`[dependencies]`（P4，构建时直接报错并说明原因），以及 `main` 组件的 `EMBED_FILES`/`EMBED_TXTFILES`、自定义 CMake 逻辑。

**门槛验证（构建部分）**：先统计 ESP-IDF v6.0.3 自带的 458 个示例：

- 192 个依赖组件管理器，属于 P4；
- 63 个在 CMake 里有自定义逻辑，包括 `EMBED_FILES`、按 target 写的分支、cmakev2 等；
- 其余 195 个可以直接转成 idf.toml 布局。

`MINIMAL_BUILD ON` 那一行（408 个示例都有）不影响构建：生成的 `main` 组件依赖全部组件，是全量构建。

转换脚本把 `main/` 拷到 `src/`，把 `sdkconfig.defaults` 及其 `.<target>` 文件转成 `[config]`，把分区表 CSV 转成 `[[partition]]`。从这 195 个里按类别抽样 33 个，用 `idf --offline --locked build` 构建，覆盖 esp32、esp32s3、esp32c6、esp32h2、esp32p4，结果全部通过：

- 第一轮暴露了一个真实问题：`Kconfig.projbuild` 里的 `rsource` 会找相对路径下的文件，复制到生成目录后就找不到了。现在改为在生成目录放一个 `source "<绝对路径>"` 的存根。
- `security/nvs_encryption_hmac` 需要在 `[config]` 里写上 `NVS_SEC_HMAC_EFUSE_KEY_ID = 0`。上游示例本身也要求手动设置这个值，它的 CI 配置里同样写了这一行。
- 所有示例构建后，项目目录里都只有 `idf.toml`、`idf.lock`、`src/`、`components/` 和 `compile_commands.json` 链接。

完整构建一个示例约 25 秒，不改代码再次构建约 0.1 秒（M 系列 Mac）。

### P2b 设备闭环（2026-11-19）

门槛已验证：在 ESP32-P4 开发板（芯片版本 v1.3，USB-Serial-JTAG，16 MB flash，macOS arm64）上，`run`、`flash`、`monitor`、`erase`、`image` 的日常用法全部可用。

- **依赖**：espflash 4.7.0（锁定 `=4.7.0`），开启 `cli` feature 只为复用它的 `ResolvingPrinter`（地址解码和 RISC-V 栈回溯），这个模块不提供 SemVer 保证。espflash 4.7 要求 rustc 1.95，`rust-version` 随之提到 1.95。新编译器对 miette `bail!` 用在表达式位置会报弃用 lint，这些地方改成了 `return Err(miette!(…))`。
- **端口**（`src/serial.rs`）：`-p` 或 `IDF_PORT` 优先；否则按 USB VID/PID 找已知的板子（Espressif 303a:\*、CP210x、CH340/CH343/CH9102、FTDI），macOS 上去掉 `/dev/tty.*` 只留 `/dev/cu.*`。只有一个就直接用，多个时在终端里提示选择，`--no-input` 或非终端时报错并列出候选。`~/.config/idf/config.toml` 还没做。
- **`flash`**（`src/commands/device.rs`）：先构建，再按 `flasher_args.json` 写入它列出的所有镜像（含 otadata 等），`--app`/`--bootloader`/`--partition-table` 只写对应的那个。镜像原样写入，只像 esptool 一样补齐到 4 字节；espflash 用 MD5 跳过板子上已经相同的镜像。写入前做 esptool 的检查：镜像头里的芯片 ID、芯片版本范围（不符合就报错，P4 v3 以下会提示 `ESP32P4_SELECTS_REV_LESS_V3`），以及镜像是否超出板子的 flash；预检失败会把芯片复位回原来的固件。默认 460800 baud。
- **复位**：espflash 在写入结束时已经按连接的 `ResetAfterOperation` 复位一次，不能再额外复位，否则第二次复位会打断刚开始的启动（P4 上表现为启动到 `app_main` 附近又出现一次 `CHIP_USB_UART_RESET`，连续复位时 ROM 甚至会 panic）。`run` 用 `NoResetNoStub` 烧录，再由 monitor 打开端口、设好波特率后复位一次，这样 UART 桥接的板子也不会丢开头的输出。打开端口时先释放 RTS 再释放 DTR：RTS 高、DTR 低就是复位，顺序反了每次打开端口都会复位一次。
- **`monitor`**（`src/monitor.rs`）：串口循环、按键和重连是自己写的，输出交给 espflash 的 `ResolvingPrinter`，ELF 为应用 ELF 加 SDK 里 `esp-rom-elfs` 工具的 ROM ELF（`ESP_ROM_ELF_DIR` 下取版本号不超过芯片版本的最大那个，和 esp-idf-monitor 一样；芯片版本取连接时读到的，单独运行 monitor 时取 sdkconfig 的 `ESP_REV_MIN_FULL`）。espflash 也内嵌了一套 ROM ELF（7.7 MB），不调用它的 `DeviceInfo::rom()` 就不会被链接进来；release 二进制从 8.3 MB 变成 9.6 MB。P4 panic 时能把栈转储还原成完整调用链，带源码行号。默认启动时复位（同 idf.py），`--no-reset` 直接接上正在运行的固件。Ctrl+R 复位，Ctrl+C 或 Ctrl+] 退出；端口消失时等它以同名回来。波特率取 `ESP_CONSOLE_UART_BAUDRATE`，`-b` 覆盖。不在项目里也能用，只是不解码。stdout 不是终端或设置了 `NO_COLOR` 时去掉颜色序列（crossterm 在关闭颜色时仍会输出空的 `ESC[m`，一并过滤）。
- **`erase`**：无参数擦整片；`--partition` 从板子上读分区表（偏移取构建的 `PARTITION_TABLE_OFFSET`，默认 0x8000，和 parttool.py 一样以板子上的表为准）；`--region` 要求 4K 对齐，连接前就检查。
- **`image`**（`src/image.rs`）：原生实现 esptool `merge-bin`（raw）、`merge-bin --format uf2`（mkuf2.py 现在调用的就是它）和 `mkdfu.py`。DFU 只支持 esp32s2/s3/p4，PID 按 mkdfu 的十六进制解析（P4 的 "12" 是 0x12），认 `ESP_DFU_PART_SIZE`。esptool 改写 bootloader 头部 flash 参数的逻辑没有移植：idf.py 传入的参数和构建时写进头部的一样，改写是空操作。差分验证：同一个 P4 构建，三种格式与 `idf native merge-bin / uf2 / dfu` 的输出逐字节一致，DFU 在 `ESP_DFU_PART_SIZE=0x10000` 分块时也一致。

留给以后：

- 端口配置文件、`monitor` 的日志落盘和时间戳、esp-idf-monitor 的 Ctrl+T 菜单。
- 拔插板子的重连路径只在逻辑上覆盖，本次没有实际拔线测试；P4 的 USB-Serial-JTAG 在芯片复位时不会掉线。
- 只在 USB-Serial-JTAG 上验证过；UART 桥接（CP210x/CH34x）的复位时序沿用 espflash 的，没有实测。
- `idf native … monitor` 被杀掉时，idf.py 的子进程不会跟着退出（交互时 Ctrl+C 发给整个进程组，不受影响）。

### P3 配置（2026-11-20）

门槛已验证：只改 `idf.toml` 就能改配置。在 hello 项目上用 `config set` 和 `config menu` 改了分区表偏移（0x9000）、flash 大小（16 MB）、tick rate、日志级别和 release 的优化选项，构建后烧到 P4 板子上，启动日志里分区表在 0x9000、flash 为 16 MB，应用正常运行；项目目录里仍只有 `idf.toml`、`idf.lock` 和 `src/`。

- **数据来源**（`src/kconfig.rs`）：菜单结构读构建目录里 kconfgen 生成的 `config/kconfig_menus.json`（类型、提示、帮助、范围、依赖、所在菜单），值读 `config/sdkconfig.json`。其他值都是按 IDF `kconfig.cmake` 的参数直接跑 kconfgen 算出来的：只给 defaults、不给 sdkconfig，也就是一次构建会从 `idf.toml` 得到的结果（`idf build` 在 defaults 变化时会把 sdkconfig 删掉重来）。一次约 0.3 s。Kconfig 自身的默认值（defaults 为空）缓存在 `config/idf-defaults.json`，`kconfig_menus.json` 更新后失效。
- **`kconfig_menus.json` 缺的信息**：提示的 `if` 条件、`select`/`imply` 的来源、带条件的多个默认值它都没有。例如 `ESP_CONSOLE_UART_BAUDRATE` 的提示是 `prompt … if ESP_CONSOLE_UART_CUSTOM`，JSON 里只有 `depends_on ESP_CONSOLE_UART`，没法解释为什么设了不生效。所以加了 `src/kconfig_why.py`（编进二进制，运行时写到临时目录，用 SDK venv 的 `python -I` 跑）：像 kconfgen 一样加载 Kconfig 树和 defaults，只对问到的符号输出提示条件和可见性、生效的 select/imply、带条件的默认值和定义位置。只在需要解释时才跑。
- **`get`**：choice 输出选中的选项名，hex 输出 `0x…`，字符串不加引号，方便脚本使用。未激活的 bool 视为 `false`，其他未激活的符号报错，并说明它依赖什么、依赖的当前值是多少。
- **`set KEY=VALUE…`**：按菜单里的类型和范围检查后写入 `[config]`；用 `-r` 或 `--profile` 显式选了 profile 时写入 `[profile.<name>.config]`。choice 写成选项，`LOG_DEFAULT_LEVEL=debug` 写成 `LOG_DEFAULT_LEVEL_DEBUG = true`，同一 choice 的其他选项从表里删掉。没有提示的符号拒绝写入。用 toml_edit 改，保留注释，覆盖已有值时也保留行尾注释。写完用 kconfgen 算一遍新结果，设了却不生效的会告诉原因（依赖没开、提示被隐藏、被 select、超出范围）。
- **`diff`**：列出和 Kconfig 默认值不同的可设置符号，分成 `idf.toml` 里写了的和由它们连带变化的两组；choice 只列选中的选项（注明替换了哪个默认选项）。输出是可以直接粘进 `idf.toml` 的 TOML。和默认值相同的 `idf.toml` 条目单独列出，可以删掉。
- **`explain`**：类型和范围、菜单路径、当前值和它的来源（`idf.toml [config]`、profile、Kconfig 默认、被 select、由其他设置决定）、默认值、依赖（带依赖符号的当前值）、提示条件、select/imply 来源、带条件的默认值（标出生效的那条）、定义所在的 Kconfig 文件和帮助。choice 列出全部选项，标出选中项和默认项。
- **`menu`**：先用 kconfgen 按 `idf.toml` 生成一份 sdkconfig，放在 `build/<target>/<profile>/menuconfig/`（menuconfig 会显示这个路径，所以不放临时目录）；用和 IDF menuconfig target 相同的环境变量运行 `esp_menuconfig`，退出后比较前后的值。改动了的可设置符号写回 `idf.toml`，choice 只写选中的选项；改回 Kconfig 默认值的条目从表里删掉（先验证删掉后结果不变，否则保留显式值）；最后用新的 `idf.toml` 再算一遍，和 menuconfig 的结果对不上的会提示。构建自己的 sdkconfig 不动，下次构建从 `idf.toml` 重新生成。esp_menuconfig 启动时会打印几十行 SDK 自身 Kconfig 的弃用 NOTE，所以通过一行 `runpy` 包装只屏蔽 `log.note`，警告和错误照常显示。
- **构建时检查**：`idf build` 每次重新配置后检查 `[config]`：未知的键（附相近的名字）、给 choice 赋值、没有提示的符号，以及值没有生效的条目，都会给出警告和原因。
- **省掉 CMake**：只有 defaults 或分区表变了时，菜单结构不会变，`config` 命令不跑 CMake configure（约 3 s），直接用 kconfgen 算值。Ninja 不会因为 defaults 变化而重新运行 CMake，所以这时删掉 `build.ninja`，让下次构建完整地重新配置。`Build::configure` 现在返回是否真的运行了。

留给以后：

- `config set` 只检查要设的值本身；如果打开一个选项会连带改变很多其他值，目前只在 `diff` 里能看到。
- 提示被隐藏的符号（如上面的波特率）`set` 照样会写入，只给警告，因为用户可能正准备同时打开它的前提。
- bootloader 子工程在 sdkconfig 变化后会自己重新跑一次 CMake，它的输出会漏到 `idf build` 的输出里（idf.py 也是这样）。
