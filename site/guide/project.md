---
title: 项目与 idf.toml
description: idf.toml 各段的写法、idf.lock 的作用，以及项目目录和 build/ 里分别放什么。
---

# 项目与 idf.toml

`idf.toml` 是项目唯一的事实来源。`idf` 从它生成 IDF 需要的 CMake 工程、`sdkconfig.defaults`、分区表和组件清单，全部放在 `build/` 里。

## 项目目录

```text
blink/
├── idf.toml                 # 手写
├── idf.lock                 # 由 idf 生成，提交到版本库
├── src/                     # main 组件的源码
├── components/              # 可选：项目自己的组件
├── build/                   # 生成物，不提交
└── compile_commands.json    # 指向最近一次构建的符号链接，不提交
```

项目里不出现 `CMakeLists.txt`、`sdkconfig`、`idf_component.yml`、`dependencies.lock` 和 `managed_components/`。`idf new` 生成的 `.gitignore` 忽略 `/build/` 和 `/compile_commands.json`。

`idf` 从当前目录（或 `-C` 指定的目录）向上查找 `idf.toml`，所以在 `src/` 的子目录里运行命令也能找到项目。

### src/

`src/` 是生成的 `main` 组件：

- 递归收集 `src/` 下扩展名为 `.c`、`.cc`、`.cpp`、`.cxx`、`.S`、`.s` 的文件，跳过以 `.` 开头的文件和目录。至少要有一个源文件，其中定义 `void app_main(void)`。
- `src/` 本身是头文件目录（`INCLUDE_DIRS`）。
- `src/Kconfig` 和 `src/Kconfig.projbuild` 会被引用进 `main` 组件，里面的 `rsource` 仍按 `src/` 的相对路径查找。

### components/

存在 `components/` 目录时，它被设为 `EXTRA_COMPONENT_DIRS`，其中每个子目录按 IDF 的常规方式作为一个组件，自带 `CMakeLists.txt`。

::: warning 暂不支持
`main` 组件的 `EMBED_FILES` / `EMBED_TXTFILES` 和自定义 CMake 逻辑目前没有对应的写法。需要这些的代码可以放进 `components/` 下的组件，用组件自己的 `CMakeLists.txt`。
:::

## idf.toml

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

顶层只允许 `[project]`、`[config]`、`[dependencies]`、`[[partition]]`、`[profile.*]` 这几段。所有表都拒绝未知的键；出错时 `idf` 会指出出错的行和列。

### [project]

| 键 | 说明 |
| --- | --- |
| `name` | 项目名，以字母开头，只含字母、数字、`-`、`_`。也是生成的 ELF、bin、map 的文件名 |
| `target` | 芯片名，如 `esp32s3`。`idf target list` 列出 ESP-IDF 6.0 的非预览芯片；其他形如 `esp32…` 的名字也接受，交给 SDK 判断 |
| `idf` | 发行系列 `"6.0"`，或确切版本 `"6.0.3"`。不接受任意的 semver 约束；超出工具支持的范围直接报错 |

`target` 可以用 `-t` 临时覆盖，用 `idf target set <chip>` 写回 `idf.toml`。`idf` 用 toml_edit 修改文件，保留注释和格式。`idf` 版本用 `idf sdk use <version>` 修改，见 [SDK 与环境](/guide/sdk)。

### [config]

Kconfig 选项，键不带 `CONFIG_` 前缀，值是布尔、整数或字符串：

```toml
[config]
FREERTOS_HZ = 1000
LOG_DEFAULT_LEVEL_DEBUG = true     # choice：写选中的那个选项
PARTITION_TABLE_OFFSET = "0x9000"  # hex 类型：写成 "0x…" 字符串
```

写了 `CONFIG_FOO` 会报错并提示去掉前缀。hex 值必须写成 `"0x…"` 字符串，因为 Kconfig 把 hex 符号的数字一律按十六进制读，`4096` 会被当成 `0x4096`。choice、类型检查和构建时的检查见 [配置](/guide/config)。

### [profile.\<name\>.config]

叠加在 `[config]` 之上的覆盖。`dev` 和 `release` 是内置的 profile，不声明也能用；默认是 `dev`，`-r` / `--release` 选 `release`，`--profile <name>` 选任意名字。自定义的名字必须在 `idf.toml` 里有对应的 `[profile.<name>]`，否则报错。profile 目前只有 `config` 一个子表。

```toml
[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true
```

每个目标和 profile 有自己的构建目录，切换 profile 不会让另一个 profile 重新配置。

### [dependencies]

组件依赖：注册表、git、本地路径三种来源，以及按芯片或 IDF 版本的条件依赖。见 [依赖](/guide/dependencies)。

### [[partition]]

分区表，用数组写，顺序就是分区的顺序。

| 键 | 说明 |
| --- | --- |
| `name` | 1–15 个 ASCII 字符，在表内唯一 |
| `type` | 如 `app`、`data` |
| `subtype` | 如 `factory`、`nvs`、`phy` |
| `size` | 大小 |
| `offset` | 可选。省略时紧跟上一个分区（按对齐） |
| `encrypted` | 可选，默认 `false` |
| `readonly` | 可选，默认 `false` |

`size` 和 `offset` 可以写整数（字节）、`"0x…"`、带 `K` 或 `M` 后缀的十进制（`"24K"`、`"1M"`）。`"0x10K"` 这种十六进制加后缀的写法不接受。

写了 `[[partition]]` 时，`idf` 生成 `partitions.csv`，并在 `sdkconfig.defaults` 末尾加上 `PARTITION_TABLE_CUSTOM=y` 和对应的文件名，所以 `idf.toml` 里的分区表总是生效。没写时使用 IDF 的默认分区表。

## idf.lock

`idf.lock` 锁定 `idf.toml` 没有确定下来的一切，与 `idf` 工具本身的版本无关。文件开头注明由 `idf` 生成，不要手改。

```toml
version = 1

[idf]
version = "6.0.3"
commit = "76f5dedd9950a3012fee8fb7d5586df21fc67802"
source = "sha256:…"          # 解压后的源码树摘要

[python]
version = "3.12.13"
exclude-newer = "2026-11-13T14:33:15Z"
constraints = "sha256:…"     # Espressif 的 constraints 文件
requirements = "sha256:…"    # 解析后带哈希的依赖集合

[[tool]]
name = "cmake"
version = "4.0.3"

[tool.archives.macos-arm64]
url = "https://github.com/Kitware/CMake/releases/download/v4.0.3/cmake-4.0.3-macos-universal.tar.gz"
sha256 = "…"
size = 80058792
```

- `[[tool]]` 为每个工具记录 macos-arm64、linux-amd64、linux-arm64 三个平台的归档，换一台机器也能用同一份 lock。
- `[[component]]` 记录第三方组件的名称、版本、来源和哈希，见 [依赖](/guide/dependencies#idf-lock-里的-component)。
- `idf.toml` 的 `idf` 与 lock 里的版本不匹配时，lock 视为过期，下一次需要 SDK 的命令会重新锁定；加了 `--locked` 时改为报错。
- 升级是显式动作：`idf sdk use` 改 IDF 版本，`idf update` 改组件版本。

各字段的完整说明见 [idf.lock 参考](/reference/idf-lock)。

## build/ 里有什么

每个目标和 profile 一个目录，`build/<target>/<profile>/`，它本身就是 CMake 的二进制目录：

```text
build/esp32s3/dev/
├── project/                  # 生成的 CMake 源目录
│   ├── CMakeLists.txt        # 设置 SDKCONFIG 等，include IDF 的 project.cmake
│   ├── main/CMakeLists.txt   # 用绝对路径列出 src/ 下的源文件
│   ├── main/idf_component.yml  # 有 [dependencies] 时
│   ├── sdkconfig.defaults    # [config] + 当前 profile
│   ├── partitions.csv        # 有 [[partition]] 时
│   ├── dependencies.lock     # 组件管理器写的锁
│   └── managed_components/   # 组件管理器下载的组件
├── sdkconfig
├── config/                   # kconfgen 输出（sdkconfig.json、kconfig_menus.json 等）
├── blink.elf  blink.bin  blink.map
├── flasher_args.json
└── ...
```

- 生成的文件内容不变就不重写，Ninja 自己判断要重编什么。
- `sdkconfig.defaults` 一变，`sdkconfig` 就被删掉重来。IDF 的 defaults 只填补 sdkconfig 里缺失的值，而 `idf.toml` 才是事实来源。
- `idf clean` 删除当前目标和 profile 的目录；`idf clean --all` 删除整个 `build/`，并移除失效的 `compile_commands.json` 链接。

::: warning 构建输出里的 git 警告
SDK 是从 release zip 解出来的，没有 `.git`，所以 CMake 输出里会出现 `git rev-parse returned 'fatal: not a git repository'` 和 `Could not use 'git describe' to determine PROJECT_VER`。这不影响构建；IDF 在没有 git 时读取 `idf` 写入的 `version.txt`。
:::
