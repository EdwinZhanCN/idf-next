---
title: SDK 与环境
description: idf sdk 的五个子命令、env 和 doctor，全局缓存 ~/.idf-next/ 的布局，以及如何离线复现同一套环境。
---

# SDK 与环境

一个 SDK 包括三部分：ESP-IDF 源码、该版本 `tools/tools.json` 里的工具链，以及该版本的 Python venv。三者一起装在 `~/.idf-next/sdk/<version>/` 下，一起删除。项目只通过 `idf.lock` 引用确切的 SDK 版本。

不需要运行 `export.sh`，也不需要激活 venv：`idf` 的每个命令都自带 SDK 的环境。

## idf sdk

### install

```bash
idf sdk install          # 在项目里：安装 idf.lock 锁定的 SDK
idf sdk install 6.0      # 安装 6.0 系列的最新版，不改动任何项目
idf sdk install 6.0.3    # 安装确切版本
```

```text
       Ready ESP-IDF 6.0.3 with Python 3.12.13 at ~/.idf-next/sdk/6.0.3
```

在项目里不带版本时，如果 `idf.lock` 与 `idf.toml` 一致，就按 lock 安装；否则在 GitHub 上解析出 `idf.toml` 要求的最新版本，安装后写入 `idf.lock`。`--locked` 下 lock 缺失或需要变更都会报错。加了 `--offline` 时不查 GitHub，只在已安装的完整 SDK 里找匹配的最新版本。

其实不必手动安装：`build`、`env`、`config` 等需要 SDK 的命令在缺少时会自动执行同样的步骤。

### use

```bash
idf sdk use 6.0.3
```

把 `idf.toml` 的 `[project] idf` 改成给定的版本（保留文件的格式和注释），重新锁定并安装。`--locked` 下拒绝修改 `idf.toml`。

### list

```bash
idf sdk list
```

```text
6.0.3      ~/.idf-next/sdk/6.0.3  (Python 3.12.13, used by this project)
```

括号里会标注安装不完整、不在本工具支持范围内、被当前项目使用等情况。

### remove

```bash
idf sdk remove 6.0.3
```

只接受确切版本（`6.0` 会报错 `give an exact version such as 6.0.0`）。只删除 `sdk/<version>/`，下载缓存、Python 解释器和 uv 缓存都保留，所以之后可以离线重装。如果当前项目的 `idf.lock` 用的就是它，会给出警告。

### path

```bash
idf sdk path           # 项目 idf.lock 锁定的版本
idf sdk path 6.0.3
```

打印该 SDK 的 `IDF_PATH`，例如 `~/.idf-next/sdk/6.0.3/esp-idf`。同样只接受确切版本。

## idf env

打印 `idf` 运行上游工具时使用的环境。需要时会先安装 SDK。

```bash
idf env
```

```text
IDF_PATH             ~/.idf-next/sdk/6.0.3/esp-idf
IDF_TOOLS_PATH       ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c
IDF_PYTHON_ENV_PATH  ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv
ESP_IDF_VERSION      6.0
OPENOCD_SCRIPTS      ~/.idf-next/sdk/6.0.3/tools/openocd-esp32/…/share/openocd/scripts
ESP_ROM_ELF_DIR      ~/.idf-next/sdk/6.0.3/tools/esp-rom-elfs/20260528/
PATH                 ~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv/bin
                     ~/.idf-next/sdk/6.0.3/tools/xtensa-esp-elf/esp-15.2.0_20251204/xtensa-esp-elf/bin
                     …
                     ~/.idf-next/sdk/6.0.3/esp-idf/tools
```

`--shell` 输出 POSIX shell 的 `export` 语句，可以在当前 shell 里直接使用 SDK 的工具：

```bash
eval "$(idf env --shell)"
```

`IDF_TOOLS_PATH` 指向 venv 所在的目录（里面放着 Espressif 的 constraints 文件），`IDF_PYTHON_ENV_PATH` 指向 venv。这样 IDF 自己的 `idf_tools.py check-python-dependencies`（CMake 每次构建前都会运行）原样通过。

## idf doctor

只读检查，不安装也不修改任何东西：

- 工具：`idf` 版本和支持的 IDF 范围、主机平台、uv、缓存目录。
- 在项目里：`idf.toml` 和 `idf.lock` 是否一致；锁定的 SDK 是否已安装、与 lock 是否一致；每个工具执行 tools.json 里的 `version_cmd`，按 `version_regex` 精确比对版本；Python 能否运行；IDF 的 `check-python-dependencies` 是否通过。
- 不在项目里：列出已安装的 SDK，标出不完整和超出支持范围的版本。

每个问题下面给出修复命令。有失败项时以非零状态退出，只有警告时正常退出。示例输出见 [快速开始](/guide/getting-started#检查环境)。

## 全局缓存布局

```text
~/.idf-next/
  sdk/<version>/                  一个 SDK，作为整体安装和删除
    esp-idf/                      源码（没有 .git，另写入 version.txt）
    tools/<tool>/<version>/       tools.json 里的工具链
    python/<digest>/              每个解析结果一个 venv
      venv/
      espidf.constraints.vX.Y.txt
      requirements.txt
      env.toml                    最后写入，标记 venv 完整
    sdk.toml                      最后写入，标记源码和工具链完整
  downloads/                      按哈希寻址的下载存储（离线重装用）
  python/                         uv 管理的 Python 解释器
  components/                     组件管理器的下载缓存，各项目共用
  uv-cache/                       uv 的 wheel 缓存（硬链接进各 venv）
```

- 根目录可以用环境变量 `IDF_NEXT_HOME` 换到别处。
- 所有写入都持有 `~/.idf-next/.lock` 文件锁。
- `sdk.toml` 和 `env.toml` 最后写入；中途中断的安装在 `sdk list` 里显示为 incomplete，再运行一次 `idf sdk install <version>` 即可补完。

## 安装时做了什么

**IDF 源码**：官方 release zip 约 2 GB，其中 1.86 GB 是 `.git`。`idf` 先读 zip 的中央目录，只用 HTTP Range 下载非 `.git` 的条目（约 130 MB），每个条目校验 CRC-32，再对整棵源码树计算摘要写进 `idf.lock` 的 `[idf] source`。

**工具链**：读取 SDK 自带的 `tools/tools.json`，对当前主机应用 `platform_overrides`，安装所有 `install: always` 的工具，另外装上 `cmake` 和 `ninja`，不依赖系统里的 CMake 和 Ninja。归档按 sha256 和大小校验，4 个线程并行下载解压。

**Python 环境**：用 uv 执行 `uv pip compile --universal --generate-hashes --exclude-newer <时间>`，输入是该 SDK 的非可选 requirements 和 Espressif 的 constraints 文件。默认解释器是 Python 3.12，只用 PyPI，不用 Espressif 的额外 pip 索引。解析结果和 constraints 文件按摘要存进下载存储；venv 目录以解析结果的摘要命名，同一 SDK 版本下解析结果不同的项目不会互相覆盖。

**镜像**：支持环境变量 `IDF_GITHUB_ASSETS`，语义与 `idf_tools.py` 相同（把 `https://github.com/…` 换成 `https://$IDF_GITHUB_ASSETS/…`）。解析 IDF 版本时调用 GitHub API，设置 `GITHUB_TOKEN` 可以避开匿名访问的速率限制。

## 离线复现

`idf.lock` 加上下载缓存，就能在不联网的情况下重建同一套 SDK：

```bash
idf sdk remove 6.0.3
idf --offline --locked sdk install
```

- `--offline`：`idf` 自己不访问网络。需要下载而缓存里没有时直接报错（`… needs the network, but --offline was given` 之类）。
- `--locked`：lock 缺失或需要变更时报错，而不是改写它。

在 macOS arm64、ESP-IDF v6.0.3 上验证过：删除后重建出的 SDK（源码、工具链、venv 共 50,559 个文件）与删除前逐字节一致，`idf.lock` 不变。Python 依赖缺失时，`idf` 用 lock 里记录的同样输入重新解析，结果的摘要必须与 lock 一致。

::: warning 已知限制
- 第三方组件不在这个保证之内：全新的构建目录第一次配置时，组件管理器仍要联网读取注册表的元数据，`--offline` 也不会阻止它。见 [依赖](/guide/dependencies#已知限制)。
- 运行上游 Python 脚本会在共享的 SDK 目录里写 `__pycache__`。不影响正确性，但 SDK 目录在使用后不再是只读的。
:::
