---
title: 配置
description: 用 idf config 读写 Kconfig 选项：值在 idf.toml 里怎么写、choice 的写法、profile，以及构建时的检查。
---

# 配置

Kconfig 配置只存在 `idf.toml` 里。`[config]` 是所有 profile 共用的，`[profile.<name>.config]` 叠加在它之上。每次构建从它们生成 `sdkconfig.defaults`；它一变，构建目录里的 `sdkconfig` 就被删掉重来，所以手改 `build/` 里的 `sdkconfig` 不会留下来。

## 在 idf.toml 里怎么写

```toml
[config]
FREERTOS_HZ = 1000                 # int
ESP_TASK_WDT_EN = false            # bool
PARTITION_TABLE_OFFSET = "0x9000"  # hex：写成 "0x…" 字符串
LOG_DEFAULT_LEVEL_DEBUG = true     # choice：写选中的选项

[profile.release.config]
COMPILER_OPTIMIZATION_PERF = true
```

| Kconfig 类型 | TOML 值 | 写进 sdkconfig.defaults |
| --- | --- | --- |
| bool | `true` / `false` | `CONFIG_X=y` / `# CONFIG_X is not set` |
| int | 整数 | `CONFIG_X=1000` |
| hex | `"0x…"` 字符串 | `CONFIG_X=0x9000` |
| string | 字符串 | `CONFIG_X="…"` |

- 键不带 `CONFIG_` 前缀，写了会报错。键必须是大写字母、数字和下划线。
- hex 一定要写成 `"0x…"` 字符串。Kconfig 把 hex 符号的数字一律按十六进制读，写整数 `4096` 会被当成 `0x4096`。
- 写了 `[[partition]]` 时，分区表相关的两项由 `idf` 追加在最后，`[config]` 里的同名设置不起作用。

### choice

choice（如日志级别、优化级别）不能直接赋值，要把选中的那个选项设为 `true`：

```toml
[config]
LOG_DEFAULT_LEVEL_DEBUG = true     # 不要写 LOG_DEFAULT_LEVEL = "debug"
```

同一个 choice 只写一个选项。用 `idf config set` 时可以写 choice 的名字加选项名，`idf` 会换算成上面的形式，并删掉同一 choice 的其他选项：

```bash
idf config set LOG_DEFAULT_LEVEL=debug
```

choice 的选项不能单独设为 `false`；要换，就设另一个选项。

## config get

打印一个符号的生效值，格式便于脚本使用：

```bash
idf config get FREERTOS_HZ             # 100
idf config get PARTITION_TABLE_OFFSET  # 0x8000
idf config get LOG_DEFAULT_LEVEL       # LOG_DEFAULT_LEVEL_INFO
```

choice 输出选中的选项名，hex 输出 `0x…`，字符串不加引号。未激活的 bool 输出 `false`；其他未激活的符号报错，并说明它依赖什么、依赖的当前值是多少。

## config set

```bash
idf config set FREERTOS_HZ=1000 LOG_DEFAULT_LEVEL=debug
```

```text
         Set FREERTOS_HZ = 1000 in [config] (was 100)
         Set LOG_DEFAULT_LEVEL_DEBUG = true in [config] (was false)
```

- 默认写入 `[config]`。显式给了 `-r` 或 `--profile <name>` 时写入 `[profile.<name>.config]`：

  ```bash
  idf -r config set COMPILER_OPTIMIZATION=perf
  ```

  ```text
           Set COMPILER_OPTIMIZATION_PERF = true in [profile.release.config] (was false)
  ```

- 按菜单里的类型和范围检查后再写。bool 接受 `true`/`false`、`y`/`n`、`yes`/`no`、`on`/`off`、`1`/`0`；int 和 hex 接受十进制或 `0x…`；超出范围报错。
- 没有提示（prompt）的符号由 Kconfig 从其他选项推出，拒绝写入。
- 用 toml_edit 修改，保留注释；覆盖已有的值时保留行尾注释。
- 写完后用 kconfgen 算一遍新结果。设了却没有生效的值会给出原因：依赖没开、提示被隐藏、被其他选项 select、超出范围。

## config diff

列出和 Kconfig 默认值不同的可设置符号。输出就是 TOML，可以直接粘进 `idf.toml`：

```bash
idf config diff
```

```text
# dev for esp32s3: 2 set in idf.toml, 0 more that follow
FREERTOS_HZ = 1000              # default 100
LOG_DEFAULT_LEVEL_DEBUG = true  # instead of LOG_DEFAULT_LEVEL_INFO
```

分成两组：`idf.toml` 里写了的，以及由它们连带变化的（`# Following from the settings above`）。choice 只列选中的选项，并注明替换了哪个默认选项。`idf.toml` 里和默认值相同的条目单独列在最后（`# Already Kconfig's defaults: …`），可以删掉。

## config explain

```bash
idf config explain ESP_CONSOLE_UART_BAUDRATE
```

```text
ESP_CONSOLE_UART_BAUDRATE  (int, 1200..=4000000)
  menu:        Component config > ESP-STDIO > UART console baud rate
  value:       115200  (Kconfig default)
  default:     115200
  depends on:  ESP_CONSOLE_UART (ESP_CONSOLE_UART = y)
  settable if: ESP_CONSOLE_UART_CUSTOM (ESP_CONSOLE_UART_CUSTOM = n)
  defaults:    74880 if IDF_TARGET_ESP32C2 && XTAL_FREQ_26
               115200  <- applies
  defined at:  esp-idf/components/esp_stdio/Kconfig:150

  This baud rate is used by both the ESP-IDF Bootloader and the app ...
```

列出类型和范围、菜单路径、当前值和它的来源（`idf.toml [config]`、profile、Kconfig 默认、被 select、由其他设置决定）、默认值、依赖（带依赖符号的当前值）、提示条件、select/imply 的来源、带条件的多个默认值（标出生效的那条）、定义所在的 Kconfig 文件和帮助文本。

上面这个例子说明了为什么光写 `ESP_CONSOLE_UART_BAUDRATE` 不生效：它的提示只在 `ESP_CONSOLE_UART_CUSTOM` 打开时可见。

对 choice，`explain` 列出全部选项，用 `*` 标出选中项，并标出默认项和来源：

```text
LOG_DEFAULT_LEVEL  (choice)
  menu:        Component config > Log > Log Level > Default log verbosity
    LOG_DEFAULT_LEVEL_NONE     No output
    LOG_DEFAULT_LEVEL_ERROR    Error
    LOG_DEFAULT_LEVEL_WARN     Warning
    LOG_DEFAULT_LEVEL_INFO     Info  (default)
  * LOG_DEFAULT_LEVEL_DEBUG    Debug  (idf.toml [config])
    LOG_DEFAULT_LEVEL_VERBOSE  Verbose
  set with:    idf config set LOG_DEFAULT_LEVEL=<OPTION>
  defined at:  esp-idf/components/log/Kconfig.level:3
```

## config menu

```bash
idf config menu
idf -r config menu     # 改动写进 [profile.release.config]
```

运行上游的 menuconfig，退出后把差异写回 `idf.toml`：

- menuconfig 编辑的是一份只由 `idf.toml` 生成的 sdkconfig，放在 `build/<target>/<profile>/menuconfig/`。构建自己的 `sdkconfig` 不受影响。
- 改动了的可设置符号写回 `idf.toml`，choice 只写选中的选项。
- 改回 Kconfig 默认值的条目从表里删掉，前提是删掉之后结果不变；否则保留显式的值。
- 写回后用新的 `idf.toml` 再算一遍，和 menuconfig 的结果对不上的符号会给出警告。
- 需要终端；stdin 或 stdout 不是终端时报错。

SDK 自身 Kconfig 的几十行弃用 NOTE 被屏蔽了，警告和错误照常显示。

## 构建时的检查

`idf build` 每次重新配置 CMake 之后检查当前 profile 的配置，对以下情况给出警告：

- 未知的键（附上相近的名字）；
- 给 choice 本身赋值（提示应该写哪个选项）；
- 没有提示的符号；
- 写了但没有生效的值。

```text
warning: idf.toml: FREERTOS_HZZ is not a Kconfig option of this build (did you mean FREERTOS_HZ?)
warning: idf.toml: LOG_COLORS = "x" has no effect (false): Kconfig settled on another value
```

这些只是警告，构建照常进行。

## 实现与开销

- 菜单结构（类型、提示、帮助、范围、依赖）读自构建目录里 kconfgen 生成的 `config/kconfig_menus.json`，值读 `config/sdkconfig.json`。
- 其他值直接按 IDF `kconfig.cmake` 的参数运行 kconfgen 算出来：只给 defaults，不给 sdkconfig，也就是下一次构建会从 `idf.toml` 得到的结果。一次约 0.3 秒。
- 只有配置值或分区表变了时，菜单结构不变，`config` 命令不运行 CMake 配置（约 3 秒）。全新的构建目录第一次运行 `config` 命令时需要先配置一次。
- `kconfig_menus.json` 里没有提示的 `if` 条件、select/imply 的来源和带条件的多个默认值。需要这些信息时（`explain`、`set` 之后的检查），`idf` 用 SDK venv 里的 Python 运行一个内置脚本，像 kconfgen 一样加载 Kconfig 树，只输出被问到的符号。

## 已知限制

::: warning
- `config set` 只检查要设的值本身。打开一个选项连带改变的其他值，目前只能在 `config diff` 里看到。
- 提示被隐藏的符号（如上面的 `ESP_CONSOLE_UART_BAUDRATE`），`set` 照样写入，只给警告，因为你可能正准备同时打开它的前提。
:::
