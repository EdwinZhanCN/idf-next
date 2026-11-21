---
title: idf.toml
description: idf.toml 的完整 schema：每个表和键的类型、是否必填、默认值和校验规则。
---

# idf.toml

`idf.toml` 是项目唯一的事实来源。项目名、芯片、ESP-IDF 版本、Kconfig 配置、依赖和分区表都写在这里；构建时 `idf` 把它翻译成 `build/` 下的 CMake 工程、`sdkconfig.defaults`、`partitions.csv` 和 `idf_component.yml`。项目目录里不需要也不应该出现这些上游文件。

本页以 `src/manifest.rs` 为准。

## 总览

| 表 | 必填 | 说明 |
| --- | --- | --- |
| [`[project]`](#project) | 是 | 项目名、芯片、ESP-IDF 版本。 |
| [`[config]`](#config) | 否 | Kconfig 配置，所有 profile 共用。 |
| [`[dependencies]`](#dependencies) | 否 | 组件依赖。 |
| [`[[partition]]`](#partition) | 否 | 分区表，按书写顺序排列。 |
| [`[profile.<name>]`](#profile) | 否 | profile 专用的配置覆盖。 |

所有表都拒绝未知的键。顶层出现其他表时报错：

```text
unknown field `build`, expected one of `project`, `config`, `dependencies`, `partition`, `profile`
```

## 完整示例

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"

[config]
FREERTOS_HZ = 1000
LOG_DEFAULT_LEVEL_DEBUG = true
ESPTOOLPY_FLASHSIZE_4MB = true

[dependencies]
"espressif/led_strip" = "^3.0.1"
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"] }
mylib = { git = "https://example.com/mylib.git", rev = "v1.0", subdir = "comp" }
local = { path = "../local" }

[[partition]]
name = "nvs"
type = "data"
subtype = "nvs"
size = "24K"

[[partition]]
name = "phy_init"
type = "data"
subtype = "phy"
size = "4K"

[[partition]]
name = "factory"
type = "app"
subtype = "factory"
offset = "64K"
size = "1M"

[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true

[profile.ci.config]
FREERTOS_HZ = 100
```

`idf new` 生成的最小文件只有 `[project]`：

```toml
[project]
name = "blink"
target = "esp32s3"
idf = "6.0"
```

## 错误信息

`idf.toml` 的每一处错误都会经 miette 指出文件、行和列：

```text
Error: idf::invalid_file

  × ESP-IDF 5.3 is not supported by this idf-next
   ╭─[/path/to/blink/idf.toml:4:7]
 3 │ target = "esp32s3"
 4 │ idf = "5.3"
   ·       ──┬──
   ·         ╰── unsupported ESP-IDF release
   ╰────
  help: idf-next 0.1.0 supports ESP-IDF >=6.0.0, <6.1.0
```

TOML 语法错误、类型错误和缺少必填键（如 ``missing field `idf` ``）的标题是 `invalid idf.toml`，具体原因在标注里。下文给出各项校验对应的原文。

`idf` 修改 `idf.toml` 的命令（`target set`、`sdk use`、`config set`、`config menu`、`add`、`remove`）都用 toml_edit 原地修改，保留注释和格式；修改后的内容先通过同样的校验才会写盘。

## [project] {#project}

| 键 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `name` | 字符串 | 是 | 项目名，用作 CMake 的 `project()` 名，因此也决定 `<name>.elf`、`<name>.map` 等产物的文件名。 |
| `target` | 字符串 | 是 | 芯片，如 `"esp32s3"`。 |
| `idf` | 字符串 | 是 | ESP-IDF 发行系列（`"6.0"`）或确切版本（`"6.0.1"`）。 |

### name

必须以 ASCII 字母开头，后面只能是 ASCII 字母、数字、`-` 和 `_`。

```text
× invalid project name
  ╰── not a valid project name
help: use letters, digits, `-` and `_`, starting with a letter
```

### target

必须是 `esp32` 后接零个或多个小写字母或数字，例如 `esp32`、`esp32s3`、`esp32c61`、`esp32p4`。

```text
"ESP32S3" is not a chip name; expected one like "esp32s3" (did you mean "esp32s3"?)
```

符合这个形式但不在已知列表（`idf target list`）里的名字也接受，由 SDK 判断是否支持。已知列表是 ESP-IDF v6.0 的非预览芯片：`esp32`、`esp32s2`、`esp32s3`、`esp32c2`、`esp32c3`、`esp32c5`、`esp32c6`、`esp32c61`、`esp32h2`、`esp32p4`。

命令行的 `-t/--target` 可以临时覆盖它，使用同样的校验规则。

### idf

只接受两种写法：

- 发行系列 `"主版本.次版本"`，如 `"6.0"`，匹配该系列的任意正式版本 `6.0.x`；
- 确切版本 `"主版本.次版本.补丁"`，如 `"6.0.1"`。

可以带 `v` 前缀（`"v6.0.1"`）。不接受 semver 约束（`"^6.0"`、`">=6.0"`）、通配符和预发布版本；预发布版本（如 `6.0.0-rc1`）不会被匹配。这是有意的：项目只跟踪一个版本，升级是显式动作，具体版本由 [`idf.lock`](/reference/idf-lock) 固定。

```text
expected an ESP-IDF release like "6.0" or "6.0.1", got "^6.0"
```

版本必须在工具支持的范围内（0.1.0 为 `>=6.0.0, <6.1.0`），否则报错 `ESP-IDF 5.3 is not supported by this idf-next`，见上面的示例。

用 `idf sdk use <版本>` 修改这个键，而不是只改文件，这样会同时安装 SDK 并更新 lock。

## [config] {#config}

Kconfig 配置。键是 Kconfig 符号名，值是该符号的值。所有 profile 共用；profile 的覆盖见 [`[profile.<name>]`](#profile)。默认为空表。

### 键

- 不带 `CONFIG_` 前缀：写 `FREERTOS_HZ`，不写 `CONFIG_FREERTOS_HZ`。
- 只能包含大写字母、数字和 `_`，不能以数字开头，不能为空。

```text
drop the CONFIG_ prefix: write "FREERTOS_HZ"
"freertos_hz" is not a Kconfig symbol; expected UPPER_SNAKE_CASE
```

`idf.toml` 本身不检查符号是否存在；这一步在构建时做（见下文）。

### 值

| TOML 类型 | 对应 Kconfig | 写入 `sdkconfig.defaults` |
| --- | --- | --- |
| 布尔 | `bool` | `true` → `CONFIG_X=y`；`false` → `# CONFIG_X is not set` |
| 整数 | `int` | `CONFIG_X=1000` |
| 形如 `"0x…"` 的字符串 | `hex` | 原样输出，如 `CONFIG_X=0x9000` |
| 其他字符串 | `string` | 加引号，`\` 和 `"` 转义，如 `CONFIG_X="say \"hi\""` |

其他类型（浮点数、数组、表）报错 `a boolean, an integer or a string`。

::: warning hex 值必须写成字符串
Kconfig 会把 `hex` 类型的值一律按十六进制读，即使没有 `0x` 前缀，所以整数 `4096` 会被当成 `0x4096`。hex 符号请写成字符串：`PARTITION_TABLE_OFFSET = "0x9000"`。`idf config set` 会自动这样写。
:::

### choice

Kconfig 的 choice 不能直接赋值，写成选中的那个选项等于 `true`：

```toml
[config]
LOG_DEFAULT_LEVEL_DEBUG = true    # 不是 LOG_DEFAULT_LEVEL = "debug"
```

`idf config set LOG_DEFAULT_LEVEL=debug` 会写成上面的形式，并删除同一 choice 的其他选项。

### 构建时检查

每次重新运行 CMake 配置后，`idf build` 检查 `[config]` 和当前 profile 的覆盖，对以下情况给出警告（不中止构建）：

- 未知的键（附相近的名字）；
- 给 choice 本身赋值；
- 没有提示（prompt）的符号，它的值由 Kconfig 根据其他选项决定；
- 写了但没有生效的值（依赖没开、提示被隐藏、被 select、超出范围），并给出原因。

### 与 sdkconfig 的关系

`[config]` 只生成 `sdkconfig.defaults`，`sdkconfig` 位于构建目录 `build/<target>/<profile>/sdkconfig`。`sdkconfig.defaults` 一旦变化，`idf build` 就删掉旧的 `sdkconfig` 重新生成，因为 IDF 的 defaults 只填补 sdkconfig 中缺失的值，而 `idf.toml` 是唯一的事实来源。

## [dependencies] {#dependencies}

组件依赖。键是组件名，值是版本要求字符串或一个表。默认为空表。

```toml
[dependencies]
"espressif/led_strip" = "*"
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"] }
mylib = { git = "https://example.com/mylib.git", rev = "v1.0", subdir = "comp" }
local = { path = "../local" }
```

### 键

- 注册表依赖写 `namespace/name`，需要加引号，因为 TOML 的裸键不允许 `/`。
- git 和路径依赖可以用任意名字，但路径依赖应与组件目录同名：组件管理器按目录名查找组件，`idf add --path` 会强制这一点。

### 值

字符串形式等价于 `{ version = "<字符串>" }`。表形式的键：

| 键 | 类型 | 说明 |
| --- | --- | --- |
| `version` | 字符串 | 注册表的版本要求，原样交给组件管理器，语法由组件管理器决定。 |
| `git` | 字符串 | git 仓库 URL。 |
| `rev` | 字符串 | git 的分支、tag 或提交。只用于 `git`。 |
| `subdir` | 字符串 | 组件在仓库里的子目录。只用于 `git`。 |
| `path` | 字符串 | 本地组件目录，相对项目根目录或绝对路径。 |
| `targets` | 芯片名数组 | 只在这些芯片上引入。不能为空数组。 |
| `idf` | 字符串 | 只在这个 ESP-IDF 版本上引入，写法同 `[project] idf`。 |

`version`、`git`、`path` 必须恰好给一个。校验错误：

| 情况 | 错误 |
| --- | --- |
| 三者都没有 | ``a dependency needs one of `version`, `git` or `path` `` |
| 给了多个 | ``use only one of `version`, `git` or `path` `` |
| `rev` 或 `subdir` 用在非 git 依赖上 | ``` `rev` and `subdir` only apply to `git` dependencies ``` |
| `version`、`git` 或 `path` 为空字符串 | ``` `version` is empty ```（相应字段名） |
| `targets = []` | ``` `targets` is empty; remove it or list at least one chip ``` |
| 其他键 | `unknown field ...` |

### 翻译成组件管理器的 manifest

构建时 `[dependencies]` 写成生成的 `build/<target>/<profile>/project/main/idf_component.yml`：

| `idf.toml` | `idf_component.yml` |
| --- | --- |
| `version = "^1.2"`（无条件时） | `"^1.2"` |
| `targets = ["esp32", "esp32s3"]` | `rules: [{ if: "target in [esp32, esp32s3]" }]` |
| `idf = "6.0"` | `rules: [{ if: "idf_version >=6.0.0,<6.1.0" }]` |
| `idf = "6.0.1"` | `rules: [{ if: "idf_version ==6.0.1" }]` |
| `git` / `rev` / `subdir` | `git` / `version` / `path`；`idf.lock` 里锁定的提交优先于 `rev` |
| `path` | 绝对路径的 `path` |

没有依赖时不生成 manifest，组件管理器不会运行。解析结果记录在 `idf.lock` 的 `[[component]]`，见 [idf.lock](/reference/idf-lock#component)。用法见 [依赖](/guide/dependencies)。

## [[partition]] {#partition}

分区表，用表数组写，以保证顺序。没有 `[[partition]]` 时使用 SDK 的默认分区表配置。

| 键 | 类型 | 必填 | 默认 | 说明 |
| --- | --- | --- | --- | --- |
| `name` | 字符串 | 是 | | 1 到 15 个 ASCII 字符，在表内唯一。 |
| `type` | 字符串 | 是 | | 如 `"app"`、`"data"`，原样写入 CSV。 |
| `subtype` | 字符串 | 是 | | 如 `"factory"`、`"nvs"`、`"phy"`，原样写入 CSV。 |
| `size` | 大小 | 是 | | 分区大小。 |
| `offset` | 大小 | 否 | 留空 | 省略时紧跟上一个分区（由 gen_esp32part 按对齐规则计算）。 |
| `encrypted` | 布尔 | 否 | `false` | CSV 的 `encrypted` 标志。 |
| `readonly` | 布尔 | 否 | `false` | CSV 的 `readonly` 标志。 |

### 大小的写法

`size` 和 `offset` 接受：

- 非负整数：`4096`；
- 十进制字符串：`"4096"`；
- 十六进制字符串：`"0x1000"`（`0x` 或 `0X`）；
- 带后缀的字符串：`"24K"`（×1024）、`"1M"`（×1048576），后缀不区分大小写。

十六进制不能带后缀（`"0x10K"` 无效）。

```text
"24Q" is not a size; use bytes, "0x…", "24K" or "1M"
size cannot be negative
```

### 名称校验

```text
× invalid partition name
  ╰── must be 1–15 ASCII characters

× duplicate partition "nvs"
  ╰── already defined on line 7
help: partition names must be unique
```

`idf.toml` 只检查名称；`type`、`subtype`、偏移和对齐由构建时 IDF 的 `gen_esp32part.py` 检查。

### 生成的文件

上面的示例生成 `build/<target>/<profile>/project/partitions.csv`：

```text
# Generated by idf from idf.toml. Do not edit.
# Name, Type, SubType, Offset, Size, Flags
nvs, data, nvs, , 0x6000,
phy_init, data, phy, , 0x1000,
factory, app, factory, 0x10000, 0x100000,
```

同时在 `sdkconfig.defaults` 的末尾加上：

```text
CONFIG_PARTITION_TABLE_CUSTOM=y
CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="partitions.csv"
```

这两行在 `[config]` 之后，所以有 `[[partition]]` 时总是使用它。

## [profile.&lt;name&gt;] {#profile}

profile 是一组叠加在 `[config]` 之上的配置。每个 profile 有自己的构建目录 `build/<target>/<profile>/`。

| 键 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `config` | 表 | 否 | 与 `[config]` 规则相同；同名的键覆盖 `[config]` 的值。 |

`[profile.<name>]` 下只允许 `config`，其他键报错 ``unknown field `foo`, expected `config` ``。

### 选择 profile

| 命令行 | profile |
| --- | --- |
| 默认 | `dev` |
| `-r` / `--release` | `release` |
| `--profile <NAME>` | `<NAME>` |

`dev` 和 `release` 是内置的，不声明也能用。其他名字必须在 `idf.toml` 里声明（可以是空表 `[profile.ci]`），否则报错：

```text
no profile named `ci`
help: Built-in profiles are `dev` and `release`; others are declared as [profile.<name>] in idf.toml.
```

::: tip release 没有隐含的配置
内置的 `release` profile 本身不改变任何 Kconfig 值，它只是一个独立的构建目录，加上你在 `[profile.release.config]` 里写的覆盖。需要的优化选项请自己写进去，例如 `COMPILER_OPTIMIZATION_PERF = true`。
:::

`idf config set` 和 `idf config menu` 在命令行显式选了 profile（`-r` 或 `--profile`）时写入 `[profile.<name>.config]`，否则写入 `[config]`。

## 不在 idf.toml 里的东西

- **串口和波特率**：与机器相关，放在 `-p`、`IDF_PORT` 或 `~/.config/idf/config.toml`，见 [环境变量与用户配置](/reference/environment)。
- **具体的 SDK 版本、工具链和依赖版本**：由 `idf.lock` 固定。
- **源文件列表**：`src/` 下所有 `.c`、`.cc`、`.cpp`、`.cxx`、`.S`、`.s` 文件（递归，跳过以 `.` 开头的文件和目录）自动进入 `main` 组件，头文件目录是 `src/`。`src/Kconfig` 和 `src/Kconfig.projbuild` 会被引用为 `main` 组件的 Kconfig。项目的 `components/` 目录（如果存在）作为额外组件目录。

::: warning 暂不支持
`main` 组件的 `EMBED_FILES` / `EMBED_TXTFILES` 和自定义 CMake 逻辑目前无法在 `idf.toml` 里表达。需要它们的代码可以放进 `components/` 下的组件，用组件自己的 `CMakeLists.txt`。
:::
