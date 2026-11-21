---
title: 依赖
description: add、remove、update、tree；注册表、git、本地路径三种依赖，按芯片和 IDF 版本的条件，idf.lock 里的 [[component]] 和 --locked。
---

# 依赖

第三方组件写在 `idf.toml` 的 `[dependencies]` 里。`idf` 把它翻译成生成的 `main` 组件的 `idf_component.yml`，由 SDK venv 里的 IDF 组件管理器求解和下载。`managed_components/` 和 `dependencies.lock` 都落在 `build/<target>/<profile>/project/` 里，项目目录里不出现；求解结果合并进 `idf.lock`。

## 三种来源

```toml
[dependencies]
# 注册表：namespace/name = 版本要求
"espressif/led_strip" = "^3.1.1"

# 注册表，带条件
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"] }

# git：可选 rev（分支、tag 或提交）和 subdir（仓库里的子目录）
pid_ctrl = { git = "https://github.com/espressif/idf-extra-components.git", subdir = "pid_ctrl" }

# 本地路径：相对项目根目录，或绝对路径
mylib = { path = "../mylib" }
```

- 值是字符串时就是注册表依赖的版本要求，原样交给组件管理器，语法以组件管理器为准。
- 写成表时，`version`、`git`、`path` 三者必须恰好有一个。`rev` 和 `subdir` 只能和 `git` 一起用。
- 本地路径依赖必须以组件所在目录的名字命名，因为组件管理器按目录名找组件。

### 条件依赖

```toml
[dependencies]
"espressif/mdns" = { version = "^1.2", targets = ["esp32", "esp32s3"], idf = "6.0" }
```

- `targets = [...]`：只在为这些芯片构建时引入。不能是空数组。
- `idf = "6.0"` 或 `"6.0.3"`：只在这个 IDF 系列（或确切版本）下引入。

它们被翻译成组件管理器的 `rules`：`target in [esp32, esp32s3]`、`idf_version >=6.0.0,<6.1.0`（确切版本是 `idf_version ==6.0.3`）。`idf add` 没有对应的参数，需要条件时直接编辑 `idf.toml`。

## idf add

```bash
idf add espressif/led_strip                 # 不写要求
idf add espressif/led_strip@^3.1            # 写明要求
idf add espressif/led_strip espressif/button
idf add mylib --path ../mylib
```

git 依赖的形式是 `idf add NAME --git URL [--rev REV]`。

```text
   Resolving dependencies
      Adding espressif/led_strip ^3.1.1
      Locked 1 dependencies in idf.lock
```

- 不写要求的注册表依赖先用 `*` 求解一次，再把写进 `idf.toml` 的要求定为 `^<解析到的版本>`。
- `--git` 和 `--path` 每次只能带一个依赖名。`--rev` 必须和 `--git` 一起用。`add` 没有 `subdir` 参数：组件在 git 仓库的子目录里时（如上面的 `pid_ctrl`），直接在 `idf.toml` 里写 `subdir`。
- `--path` 的路径在项目内时写成相对项目根目录的路径，否则写成绝对路径。

## idf remove

```bash
idf remove espressif/button
```

从 `idf.toml` 删除，并重新求解，把 `idf.lock` 里不再需要的组件一起清掉。名字不在 `[dependencies]` 里时报错并列出现有的依赖。

## idf update

```bash
idf update                      # 全部
idf update espressif/led_strip  # 只更新这些
```

在 `idf.toml` 的约束范围内重新求解。给了名字时，只丢掉这些依赖以及只被它们带进来的依赖的锁，其他依赖保持原来的版本。结束时报告每个组件的版本变化，没有变化时输出 `Unchanged everything is up to date`。

## idf tree

```bash
idf tree
```

```text
hello (ESP-IDF 6.0.3)
├── espressif/button 4.2.3
│   └── espressif/cmake_utilities 1.4.1
└── espressif/led_strip 3.1.1
```

读上一次配置的求解结果；还没有时先求解。`--offline` 下没有现成结果时报错，不求解。重复出现且有子依赖的组件标 `(*)`，不再展开。

`add`、`remove`、`update`、`tree` 都不运行 CMake，直接调用组件管理器的 `prepare_components`。

## idf.lock 里的 [[component]]

```toml
[[component]]
name = "espressif/led_strip"
version = "3.1.1"
source = "registry+https://components.espressif.com/"
hash = "sha256:027f68f2b57b0dca1d68dff6cc1139d9093825aa808c2bc3ca42bb2ce530fce9"
```

| 来源 | `source` 的写法 | 说明 |
| --- | --- | --- |
| 注册表 | `registry+<URL>` | 带 `hash`（组件管理器的 `component_hash`） |
| git | `git+<URL>#<COMMIT>` | 锁定到提交 |
| 本地路径 | `path+<PATH>` | 版本为 `*`；路径在项目内时相对项目根目录 |

锁是这样生效的：

- **注册表**：全新的构建目录会先按 `idf.lock` 写一份 `dependencies.lock`（不带 `manifest_hash`）。组件管理器因此会重新求解，但以这些版本为起点，所以版本和哈希不会漂移。
- **git**：组件管理器每次求解都会重新解析分支，所以 `idf` 把锁里的提交写成生成的 manifest 里的 `version`。`idf.toml` 里的 `rev` 不变。
- **本地路径**：从磁盘读取，不需要锁。

组件管理器的下载缓存设在 `~/.idf-next/components/`（环境变量 `IDF_COMPONENT_CACHE_PATH`），各项目共用。没有任何依赖时不写 manifest，组件管理器也不运行（`IDF_COMPONENT_MANAGER=0`），不拖慢配置。

## --locked

```bash
idf --locked build
```

- `build`：配置后，如果合并求解结果会改变 `idf.lock`，直接报错，提示运行 `idf update`（或不加 `--locked` 构建）并提交 `idf.lock`。
- `add`、`remove`、`update`：会修改 `idf.toml` 和 `idf.lock`，加了 `--locked` 直接拒绝：

```text
Error:   × `idf add` would change idf.toml and idf.lock, but --locked was given
```

求解失败时，`idf` 去掉组件管理器输出里的 NOTE、进度点和 “SDK 没有 .git” 的警告；`-v` 时显示全文。

## 已知限制

::: warning 修改依赖后需要重新配置
在已经构建过的目录里执行 `idf add` 之后，下一次 `idf build` 没有重新运行 CMake 配置，新加的组件不会进入构建（实测：`build.ninja` 里没有新组件，构建 0.2 秒结束）。删除构建目录后再构建可以绕过：

```bash
idf clean
idf build
```
:::

::: warning
- 全新的构建目录第一次配置仍要联网：没有 `manifest_hash` 就要求解，组件管理器要读注册表的元数据。真正的离线复现需要 port 求解器或自己算 `manifest_hash`，目前都没有做。
- `--offline` 只约束 `idf` 自己的网络访问，不会传给组件管理器。
- `add`、`update`、`tree` 求解时给组件管理器的是上一次配置留下的 `config/sdkconfig.json`。还没配置过就没有，依赖 manifest 里按 Kconfig 取值的 `matches` 规则不生效；配置之后改了 `[config]`，这个文件在下次构建前是旧的。
- 有 `targets` 限制的依赖时，`idf.lock` 是各芯片的并集：换芯片构建只增补条目，不会清掉别的芯片的条目。
- 组件管理器的 git 缓存对 `file://` 本地仓库有问题（新提交报 `not a tree object`），真实的 https 仓库没有这个问题。
:::
