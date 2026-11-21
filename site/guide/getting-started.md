---
title: 快速开始
description: 安装 idf，创建第一个项目，构建、烧录并打开串口监视。
---

# 快速开始

本页从安装开始，走一遍 `new`、`build`、`run`。

## 前提

- **主机平台**：macOS arm64、Linux x86_64 或 Linux aarch64。其他平台上 `idf` 会直接报错 `unsupported host`。
- **uv**：`idf` 用 uv 为每个 SDK 版本创建 Python 环境。查找顺序是环境变量 `IDF_NEXT_UV`、`idf` 真实路径所在目录里的 `uv`、`PATH` 里的 `uv`。
- **ESP-IDF 版本**：本版本的 `idf` 支持 ESP-IDF `>=6.0.0, <6.1.0`。SDK 不需要你自己下载，见 [SDK 与环境](/guide/sdk)。

## 安装

从源码安装需要 Rust 1.95 或更新的版本：

```bash
cargo install --git https://github.com/EdwinZhanCN/idf-next
```

::: tip 打包方式
仓库里已经配置了 maturin（`bindings = "bin"`），把 `idf` 打成平台 wheel，唯一的 Python 依赖是 `uv`，用 `uv tool install idf-next` 安装时 `uv` 和 `idf` 装在同一个 bin 目录里。发布 workflow 在打 `v*` tag 时把 wheel 发布到 PyPI；在那之前请用上面的源码安装。
:::

确认版本和支持的 IDF 范围：

```bash
idf --version
```

```text
idf 0.1.0
supported ESP-IDF: >=6.0.0, <6.1.0
```

`idf -V` 只输出第一行。在终端里运行 `idf --version` 时还会在左侧画出 Espressif 标志；输出到管道、`TERM=dumb` 或加了 `-q` 时不画。

## 创建项目

```bash
idf new blink -t esp32s3
cd blink
```

```text
     Created blink for esp32s3 (ESP-IDF 6.0) in blink
```

`idf new <path>` 要求目录不存在；在已有目录里用 `idf init`，它不会覆盖已有的 `src/`。不给 `-t` 时芯片默认是 `esp32`。项目名取目录名，必须以字母开头，只能包含字母、数字、`-` 和 `_`。

生成的文件只有三个：

```text
blink/
├── .gitignore      # /build/ 和 /compile_commands.json
├── idf.toml
└── src/
    └── main.c
```

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"
```

`idf = "6.0"` 表示 6.0 系列的最新发布版。第一次需要 SDK 时，`idf` 把解析到的确切版本写进 `idf.lock`。`idf.toml` 的完整说明见 [项目与 idf.toml](/guide/project)。

## 构建

```bash
idf build
```

第一次构建会先解析并安装 SDK（IDF 源码、工具链、Python 环境），需要联网。之后的输出类似：

```text
      Locked ESP-IDF 6.0.3 in idf.lock
 Configuring dev for esp32s3
    Building blink for esp32s3 (dev)
    ...
    Finished build/esp32s3/dev/blink.bin (160.41 KiB) in 28.8s
```

CMake 配置阶段的输出被收起，只在失败或加 `-v` 时显示；Ninja 的输出直接透传。不改代码再次构建约 0.1 秒：

```text
    Finished build/esp32s3/dev/blink.bin (160.41 KiB) in 0.1s
```

构建之后，项目根目录多出 `idf.lock`、`build/` 和一个指向本次构建的 `compile_commands.json` 符号链接（给 clangd 等工具用）。`idf.lock` 应该提交到版本库。

## 检查环境

```bash
idf doctor
```

```text
Tool
  ✓ idf-next 0.1.0 (supports ESP-IDF >=6.0.0, <6.1.0)
  ✓ host platform macos-arm64
  ✓ uv 0.12.1 (...)
  ✓ SDK cache ~/.idf-next
Project (/path/to/blink)
  ✓ idf.toml: blink for esp32s3, ESP-IDF 6.0
  ✓ idf.lock: ESP-IDF 6.0.3 (76f5dedd99), Python 3.12.13
SDK
  ✓ ESP-IDF 6.0.3 at ~/.idf-next/sdk/6.0.3/esp-idf
  ✓ xtensa-esp-elf esp-15.2.0_20251204
  ...
Python
  ✓ Python 3.12.13 (~/.idf-next/sdk/6.0.3/python/14645654ec2ff38c/venv)
  ✓ ESP-IDF's check-python-dependencies passes

No problems found.
```

`doctor` 只读，不安装任何东西。发现问题时每一项下面会给出修复命令，并以非零状态退出。

## 烧录并监视

把开发板用 USB 接上：

```bash
idf run
```

`run` 依次构建、烧录、打开串口监视。串口按 USB VID/PID 自动识别；只接了一块板子时直接使用，多块时在终端里让你选择。监视器里 Ctrl+R 复位芯片，Ctrl+C 或 Ctrl+] 退出。

也可以分开执行：

```bash
idf flash      # 构建并烧录
idf monitor    # 只打开串口监视
```

端口、波特率和复位行为的细节见 [构建、烧录与监视](/guide/build-flash-monitor)。

::: warning ESP32-P4 v3 以下
ESP-IDF 默认按 ESP32-P4 v3.x 构建。芯片版本低于 v3 时，`idf flash` 会在写入前报错并提示在 `idf.toml` 里加上：

```toml
[config]
ESP32P4_SELECTS_REV_LESS_V3 = true
```
:::

## 常用全局选项

全局选项写在命令名之前或之后都可以（例外：`size`、`efuse`、`secure`、`coredump`、`debug`、`native` 会把命令名之后的所有参数原样交给上游工具，全局选项要写在命令名之前）。

| 选项 | 作用 |
| --- | --- |
| `-C, --project <PATH>` | 在 `PATH` 里运行，如同在那里启动 |
| `-t, --target <CHIP>` | 本次调用改用另一颗芯片，不改 `idf.toml` |
| `-p, --port <PORT>` | 串口，也可以用环境变量 `IDF_PORT` |
| `-r, --release` / `--profile <NAME>` | 选择 profile，默认是 `dev` |
| `--offline` | `idf` 自己不访问网络 |
| `--locked` | 需要修改 `idf.lock` 时直接报错 |
| `--no-input` | 从不提示输入 |
| `-v` / `-q` | 更多 / 更少输出 |

## 下一步

- [项目与 idf.toml](/guide/project)：`idf.toml` 各段的写法，`build/` 里有什么。
- [配置](/guide/config)：用 `idf config` 改 Kconfig 选项。
- [依赖](/guide/dependencies)：添加注册表、git 或本地组件。
