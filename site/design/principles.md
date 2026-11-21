---
title: 设计原则
description: idf-next 的目标与不变量、委托上游与原生实现的取舍、构建目录布局的理由，以及风险与应对。
---

# 设计原则

本页整理 `docs/PLAN.md` 中的设计决定，并对照当前代码说明每一条落实到了什么程度。

## 目标

idf-next 是一个自用的 ESP-IDF 工具链：单个 Rust 二进制 `idf`，仓库里只留 `idf.toml`、`idf.lock` 和源码。高频的日常路径用 Rust 原生实现，其余委托给上游 Python 工具。Python 环境由 `idf` 托管，用户看不到它。

两条明确的「不做」：

- **不兼容老项目**。没有 `CMakeLists.txt` 的导入模式，也不读项目目录里的 `sdkconfig`。
- **不追上游版本节奏**。目前只支持 ESP-IDF `>=6.0.0, <6.1.0`，范围写在代码里（`src/idf_support.rs`）。

## 不变量

任何实现决策都不能破坏下面九条。

| # | 不变量 | 现状 |
| --- | --- | --- |
| 1 | **项目目录**只有 `idf.toml`、`idf.lock`、`src/`，以及可选的 `components/`。不出现 `CMakeLists.txt`、`sdkconfig`、`idf_component.yml`、`dependencies.lock`、`managed_components/`。 | 成立。上游文件全部生成在 `build/` 里。此外项目根目录会有 `build/`、指向最近一次构建的 `compile_commands.json` 符号链接，以及 `idf new` 写的 `.gitignore`（它忽略前两者）。 |
| 2 | **外部依赖**：ESP-IDF SDK 源码、CMake、Ninja、交叉 GCC，以及该 SDK 版本对应的 Python venv。调试另需 OpenOCD 和 GDB。 | 成立。全部由 `idf sdk install` 装进全局缓存，不依赖系统里的 CMake 和 Ninja。 |
| 3 | **工具与 SDK 解耦**：idf-next 有独立的版本号，代码里声明支持的 IDF 范围。Python 依赖属于 SDK，不打包进工具。 | 成立。`idf --version` 报告支持范围；`idf.toml`、`idf.lock`、`idf sdk install`、`idf doctor` 都按这个范围检查。 |
| 4 | **Python 不可见**：每个 SDK 版本的 venv 由工具附带的 uv 创建，装在 `~/.idf-next/sdk/<version>/` 下。不需要激活环境，也不需要 `export.sh`。 | 成立。每个调用上游工具的命令都自己设置环境；`idf env` 只用于在 `idf` 之外调用工具。 |
| 5 | **原生与委托的分界**：高频、对延迟敏感、输出需要好看的功能原生实现；低频、高风险或体量大的功能委托上游。`idf native` 随时可以直通 `idf.py`。 | 成立，见下文。 |
| 6 | **SDK 不打补丁**：IDF 的 CMake 框架和构建期脚本原样运行。 | 成立。生成的 `CMakeLists.txt` 原样 include IDF 的 `project.cmake`。源码里补的 `version.txt` 不算补丁：IDF 在没有 `.git` 时本来就读它。 |
| 7 | **单一版本锁定**：`idf.lock` 锁定 IDF 提交、工具链哈希、Python 版本和依赖解析的哈希，与工具版本无关。升级是显式动作。 | 成立，见 [idf.lock](/reference/idf-lock)。 |
| 8 | **产物隔离**：生成物全部放在 `build/`，SDK 及其工具链和 venv 放在全局缓存。 | 基本成立。一个例外：运行上游 Python 脚本会在共享的 SDK 目录里写 `__pycache__`，不影响正确性，但 SDK 目录在使用后不再只读。 |
| 9 | **正确性定义**：原生实现的部分，输出必须与被它替代的上游工具逐字节一致。 | 对 `image` 做过差分验证（三种格式与 `idf native merge-bin / uf2 / dfu` 逐字节一致）；SDK 离线重装与删除前逐字节一致。 |

### 由不变量推出的 CLI 决定

- **没有全局 `--idf-version`**。每次调用都能临时换 SDK 版本会让 `idf.lock` 形同虚设。版本只在两个地方指定：`idf.toml` 的 `[project] idf`（`idf new` 写入）和 `idf sdk use <版本>`。
- **`[project] idf` 只接受 `"6.0"` 或 `"6.0.1"`**，不接受 semver 约束。项目跟踪一个发行系列，具体版本由 lock 固定。
- **`-t/--target` 是一次性覆盖，`idf target set` 写入 `idf.toml`**。优先级为命令行高于 `idf.toml`。
- **串口不进 `idf.toml`**。串口与机器相关，来自 `-p`、`IDF_PORT` 或 `~/.config/idf/config.toml`，默认按 USB VID/PID 探测。
- **`--offline` 之外还有 `--locked`**。前者禁止网络，后者禁止改写 lock，两者都需要才能保证可复现。
- **`new <path>` 和 `init` 分开**，沿用 Cargo 的语义。
- **`native` 是逃生口**，在托管环境里运行 `idf.py`，并带上与 `idf build` 相同的构建目录，所以两者操作的是同一个工程。

## 委托上游与原生实现

### 判断标准

- **原生实现**：高频、对延迟敏感、输出需要好看。日常循环（构建、烧录、监视）每天跑很多次，启动开销和输出质量都直接可感。
- **委托上游**：低频、高风险或体量大。`espefuse` 烧错会永久损坏硬件；组件管理器、Kconfig 求值、ldgen 体量大且跟着 IDF 演进，自己实现等于长期跟踪上游。
- **委托时只做薄封装**：`idf` 只负责补齐参数和透传输出，不解析给人看的文本。需要结构化数据时用上游的 JSON 输出（kconfgen 的 `json` 和 `json_menus`、`esp-idf-size --format json2`、组件管理器的 `dependencies.lock` 经 SDK venv 里的 ruamel.yaml 转成 JSON）。这样上游改了显示格式不会让 `idf` 坏掉。

### 现状

| 功能 | 策略 | 实现 |
| --- | --- | --- |
| SDK、工具链、venv 安装 | 原生 | 自己读 `tools.json` 下载解压；IDF 源码按 HTTP Range 只下载 release zip 中非 `.git` 的条目；venv 用附带的 uv 创建。 |
| 构建驱动 | 原生编排，构建本身委托 | `idf` 生成 CMake 工程并调用 CMake 和 Ninja；IDF 的 CMake 框架和构建期脚本（kconfgen、ldgen 等）原样运行在托管 venv 里。 |
| 烧录 | 原生 | espflash 4.7.0（锁定 `=4.7.0`）。按 `flasher_args.json` 写入，加上 esptool 的写入前检查。 |
| 串口监视 | 原生 | 串口循环、按键、重连自己写；地址和栈回溯解码复用 espflash 的 `ResolvingPrinter`。 |
| 擦除 | 原生 | espflash；分区表用 esp-idf-part 解析。 |
| 镜像导出（merge-bin、UF2、DFU） | 原生 | 体量小，自己实现，与上游逐字节一致。 |
| 调试编排 | 委托 | `idf.py openocd gdb`；`idf` 只在检测到内置 USB-Serial-JTAG 时补 OpenOCD 参数。 |
| Kconfig 求值 | 委托 | kconfgen。`config get / set / diff / explain` 的逻辑是自己的，数据来自 kconfgen 的 JSON 输出，另加一个编进二进制的小脚本 `kconfig_why.py` 解释 JSON 里缺的信息（提示条件、select / imply 来源、带条件的默认值）。 |
| menuconfig | 委托 | 上游 `esp_menuconfig`，退出后把差异写回 `idf.toml`。 |
| 组件管理 | 委托求解 | 上游组件管理器求解和下载；`idf` 负责把 `[dependencies]` 翻译成 manifest，把结果合并进 `idf.lock`，并在新构建目录里按 lock 预置。 |
| size、coredump、eFuse、安全 | 委托 | `python -m <工具>`，补齐芯片、串口、ELF 和 map 路径，退出码原样传回。 |

### 何时才 port {#when-to-port}

P5 之后不再按计划推进，只在某个委托的工具真正碍事时（慢、输出难看、经常出问题）才把它搬过来，并用差分测试保证与上游一致。最可能的两个候选：

- **Kconfig 求值**：让 `config` 命令不必等 CMake 配置。目前已经尽量绕开 CMake（只有 defaults 或分区表变化时直接调用 kconfgen，约 0.3 秒），但第一次仍需配置一次（约 3 秒）。
- **依赖求解器**：让第三方组件进入全局缓存，并让全新构建目录离线配置成为可能。

eFuse 和安全工具明确不 port：风险高、频率低，委托上游并加确认更稳妥。

## 构建目录布局

```text
build/
  <target>/
    <profile>/                 CMake 的二进制目录（sdkconfig、build.ninja、产物）
      project/                 生成的 CMake 源目录，组件管理器眼中的「项目目录」
        CMakeLists.txt
        main/CMakeLists.txt
        main/idf_component.yml 有依赖时生成
        sdkconfig.defaults
        partitions.csv         有 [[partition]] 时生成
        dependencies.lock      组件管理器写入
        managed_components/    组件管理器写入
      menuconfig/              idf config menu 的工作目录
      config/                  kconfgen 的 JSON 输出
```

为什么这样布局：

- **按芯片隔离**（`build/<target>/`）：切换芯片不会触发全量重新配置，`-t` 临时换芯片构建也不会破坏原来的构建。
- **按 profile 隔离**（`<profile>/`）：`dev` 和 `release` 各有自己的 sdkconfig 和产物，来回切换都是增量构建。
- **profile 目录本身就是 CMake 的二进制目录**，生成的源目录 `project/` 放在它里面：一次构建的所有东西都在一个目录里，`idf clean` 删掉它就干净了。
- **组件管理器把 `project/` 当作项目目录**，所以 `managed_components/` 和 `dependencies.lock` 自然落在构建目录里。P4 原本担心这两样无法移出项目目录、必须自己实现求解器；这个布局让门槛在不 port 求解器的情况下就成立了。
- **`idf native` 和 `idf debug` 用同一对目录**（`-C build/<t>/<p>/project -B build/<t>/<p>`），与 `idf build` 操作同一个工程。
- **`sdkconfig` 只在构建目录里**。`sdkconfig.defaults` 一变就删掉 `sdkconfig` 重来，因为 IDF 的 defaults 只填补缺失的值，而 `idf.toml` 是唯一的事实来源。

增量构建的规则：生成的文件内容不变就不重写，Ninja 自己判断要重新编译什么；任何生成文件变了或还没有 `build.ninja` 才重新运行 CMake 配置。不改代码再次构建约 0.1 秒（M 系列 Mac）。

## 全局缓存与版本

- `~/.idf-next/sdk/<version>/` 存放一个 SDK 的全部内容（源码、工具链、venv），一起安装、一起删除。
- 下载按哈希寻址存放，uv 的缓存以硬链接复用包文件，所以多个 SDK 版本并存几乎不占额外空间，删除 SDK 后也能离线重装。
- SDK 的安装和删除持有 `~/.idf-next/.lock` 文件锁，多个 `idf` 同时安装时后来者等待。
- 工具本身由 `uv tool install` / `uv tool upgrade` 管理，与 SDK 互不影响：升级工具不动 SDK，切换 SDK 不需要重装工具。项目只通过 `idf.lock` 引用具体的 SDK 版本。

## 风险与应对

改成委托上游之后，主要风险从「port 是否正确」变成了两件事：托管环境能否复现，上游写进项目目录的文件能否藏进 `build/`。

| 风险 | 影响 | 应对 | 现状 |
| --- | --- | --- | --- |
| SDK venv 无法复现 | 上游的 pip 依赖漂移 | 用附带的 uv 按该版本的 constraints 和固定的 `--exclude-newer` 解析，解析结果的哈希写进 `idf.lock` | P1 已验证：删除后离线重装逐字节一致 |
| 工具与 SDK 版本不兼容 | 新工具生成的工程不适用于旧 SDK | 代码里声明支持范围，`doctor` 检查，超出范围直接报错 | 已实现 |
| `native` 的环境检查 | `idf.py` 启动时校验 Python 环境，可能拒绝运行 | 用环境变量指向 SDK venv | 已解决：`IDF_TOOLS_PATH` 和 `IDF_PYTHON_ENV_PATH` |
| `managed_components` 的位置 | 可能无法移出项目目录 | 不行就 port 求解器 | 已解决：生成的工程目录就是组件管理器的项目目录 |
| 解析上游工具的输出 | 上游一改格式就会坏 | 只传参和透传输出，需要结构化数据时用 JSON | 遵守 |
| v6.0 的 Build System v2 是预览版 | 后续版本可能改变 CMake 入口 | 先基于 v1，等 v2 稳定后再评估 | 基于 v1 |
| eFuse 烧写不可逆 | 硬件永久损坏 | 委托 `espefuse`，`idf` 只额外加确认 | 真芯片上烧写需手动输入 `burn`，拒绝 `--do-not-confirm`；烧写命令名单由测试对照 `espefuse --help` |

## 开放问题

- 用户自定义组件的配置项写在 `idf.toml` 里由工具翻译成 Kconfig，还是允许组件保留自己的 Kconfig 文件？目前 `src/Kconfig`、`src/Kconfig.projbuild` 和 `components/` 下组件自己的 Kconfig 文件都可以用。
- 默认支持哪些芯片？测试矩阵的规模取决于这个答案。目前接受任何 `esp32*` 形式的名字，已知列表是 ESP-IDF v6.0 的非预览芯片。
