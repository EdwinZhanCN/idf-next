---
layout: home
title: idf-next
description: A single-binary command-line tool that drives ESP-IDF projects from one idf.toml

hero:
  name: idf-next
  text: Drive ESP-IDF projects from one idf.toml
  tagline: A single Rust binary, idf. It manages the SDK, toolchains, and Python environment, so the project directory holds only idf.toml, idf.lock, and source files. Supports ESP-IDF >=6.0.0, <6.1.0.
  image:
    src: /logo.svg
    alt: idf-next
  actions:
    - theme: brand
      text: Quick start
      link: /en/guide/getting-started
    - theme: alt
      text: Architecture
      link: /en/design/architecture

features:
  - title: The project directory has only a few files
    details: The project contains only idf.toml, idf.lock, src/, and an optional components/. CMakeLists.txt, sdkconfig, idf_component.yml, and managed_components/ are all generated under build/&lt;target&gt;/&lt;profile&gt;/, and IDF's CMake framework runs unchanged.
    link: /en/guide/project
    linkText: Project and idf.toml
  - title: Install the SDK in one step, no export.sh needed
    details: idf sdk install puts the IDF source, the toolchains listed in tools.json (plus CMake and Ninja), and a Python venv created with uv into ~/.idf-next/sdk/&lt;version&gt;/. Commands set up the environment when they run, so no activation is needed.
    link: /en/guide/sdk
    linkText: SDK and environment
  - title: idf.lock pins the whole environment
    details: It pins the IDF commit and source tree digest, the sha256 of the toolchain archives for three host platforms, a digest of the Python dependency resolution, and the versions and hashes of third-party components. After you delete the SDK, --offline --locked rebuilds a byte-identical environment from the download cache.
    link: /en/guide/sdk#offline
    linkText: Offline reproduction
  - title: Native flash, monitor, and image export
    details: flash, monitor, run, erase, and image are built on espflash. The board is found automatically by USB VID/PID. monitor decodes addresses and stack backtraces using the application ELF and the ROM ELF, and reconnects on the same port after the board is unplugged and plugged back in. The bin, UF2, and DFU files exported by image are byte-identical to the output of the upstream tools.
    link: /en/guide/build-flash-monitor
    linkText: Build, flash, and monitor
  - title: Kconfig options are written in idf.toml
    details: config get, set, diff, and explain read the output of kconfgen directly. config menu runs the upstream menuconfig and writes the changes back to idf.toml when it exits. At build time, entries in [config] that are misspelled, have no prompt, or have no effect are reported.
    link: /en/guide/config
    linkText: Configuration
  - title: Thin wrappers around upstream tools
    details: size, efuse, secure, coredump, and debug call the upstream tools in the SDK and fill in the chip, port, map, and ELF paths automatically. On a real chip, you must type burn manually in the terminal before you burn eFuses.
    link: /en/guide/debug-security
    linkText: Debugging and security
---
