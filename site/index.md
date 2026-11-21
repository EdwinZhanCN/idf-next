---
layout: home
title: idf-next
description: 用一个 idf.toml 驱动 ESP-IDF 项目的单二进制命令行工具

hero:
  name: idf-next
  text: 用一个 idf.toml 驱动 ESP-IDF 项目
  tagline: 单个 Rust 二进制 idf。SDK、工具链和 Python 环境由它托管，项目目录里只留 idf.toml、idf.lock 和源码。支持 ESP-IDF >=6.0.0, <6.1.0。
  image:
    src: /logo.svg
    alt: idf-next
  actions:
    - theme: brand
      text: 快速开始
      link: /guide/getting-started
    - theme: alt
      text: 架构
      link: /design/architecture

features:
  - title: 项目目录只有几个文件
    details: 项目里只有 idf.toml、idf.lock、src/ 和可选的 components/。CMakeLists.txt、sdkconfig、idf_component.yml、managed_components/ 全部生成在 build/&lt;target&gt;/&lt;profile&gt;/ 下，IDF 的 CMake 框架原样运行。
    link: /guide/project
    linkText: 项目与 idf.toml
  - title: SDK 一次装齐，不需要 export.sh
    details: idf sdk install 把 IDF 源码、tools.json 里的工具链（另加 CMake 和 Ninja）和用 uv 创建的 Python venv 装进 ~/.idf-next/sdk/&lt;version&gt;/。命令运行时自动带上环境，不需要激活。
    link: /guide/sdk
    linkText: SDK 与环境
  - title: idf.lock 锁定整套环境
    details: 锁定 IDF commit 和源码树摘要、三个主机平台的工具链归档 sha256、Python 依赖解析结果的摘要，以及第三方组件的版本和哈希。删除 SDK 后用 --offline --locked 可以从下载缓存重建出逐字节一致的环境。
    link: /guide/sdk#离线复现
    linkText: 离线复现
  - title: 原生烧录、监视和镜像导出
    details: 基于 espflash 实现 flash、monitor、run、erase、image。按 USB VID/PID 自动找板子；monitor 用应用 ELF 和 ROM ELF 解码地址和栈回溯，拔插后在同名端口自动重连；image 导出的 bin、UF2、DFU 与上游工具输出逐字节一致。
    link: /guide/build-flash-monitor
    linkText: 构建、烧录与监视
  - title: Kconfig 配置写在 idf.toml 里
    details: config get、set、diff、explain 直接读取 kconfgen 的结果；config menu 运行上游 menuconfig，退出后把改动写回 idf.toml。构建时检查 [config] 里拼错、没有提示或没有生效的条目。
    link: /guide/config
    linkText: 配置
  - title: 上游工具的薄封装
    details: size、efuse、secure、coredump、debug 调用 SDK 里的上游工具，自动补上芯片、串口、map 和 ELF 路径。真芯片上烧写 eFuse 前必须在终端里手动输入 burn。
    link: /guide/debug-security
    linkText: 调试与安全
---
