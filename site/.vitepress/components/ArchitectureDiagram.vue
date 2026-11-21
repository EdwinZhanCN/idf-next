<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vitepress'

const { lang } = useData()

const zh = {
  caption: '`idf` 是唯一入口：它把 idf.toml 翻译成 build/ 里的 CMake 工程，上游的构建脚本跑在它托管的 Python 环境里。',
  project: { title: '项目目录', sub: '你写和提交的文件', items: ['idf.toml', 'idf.lock', 'src/'] },
  user: { title: '用户配置', sub: '可选', items: ['~/.config/idf/config.toml'] },
  idf: {
    title: 'idf',
    sub: 'Rust 单二进制',
    groups: [
      { name: '项目', mods: ['manifest', 'project', 'lockfile', 'kconfig'] },
      { name: '环境', mods: ['sdk', 'python', 'idf_source', 'tools_json'] },
      { name: '构建', mods: ['build', 'deps', 'firmware'] },
      { name: '设备', mods: ['serial', 'monitor', 'image', 'espflash'] },
      { name: '薄封装', mods: ['size', 'efuse', 'secure', 'coredump', 'debug'] },
    ],
  },
  cache: {
    title: '全局缓存',
    sub: '~/.idf-next/',
    items: ['sdk/<版本>/ 源码 · 工具链 · venv', 'components/ 组件下载', 'downloads/ 内容寻址的归档'],
  },
  build: {
    title: 'build/<芯片>/<profile>/',
    sub: '生成的工程与产物',
    items: ['project/ 生成的 CMakeLists · main · managed_components', 'sdkconfig · config/', '固件 .bin · .elf · .map'],
  },
  cmake: { title: 'CMake + Ninja', sub: 'IDF 的 CMake 框架原样保留', items: ['GCC 编译链接'] },
  py: {
    title: '托管 Python 环境',
    sub: 'SDK 的 uv venv',
    items: ['kconfgen · ldgen · 构建期脚本', '组件管理器', 'esptool · espefuse · espsecure'],
  },
  device: { title: '开发板', sub: 'USB-Serial-JTAG · UART 桥接', items: ['flash · monitor · erase', 'OpenOCD + GDB'] },
  edges: {
    read: '读取',
    install: '安装 / 复现',
    generate: '生成',
    configure: '配置时调用 PYTHON=venv',
    deps: '依赖求解 → idf.lock',
    flash: '烧录 · 监视（原生）',
    upstream: '委托上游',
  },
}

const en = {
  caption: '`idf` is the only entry point: it translates idf.toml into a CMake project under build/, and the upstream build scripts run in a Python environment it manages.',
  project: { title: 'Project directory', sub: 'files you write and commit', items: ['idf.toml', 'idf.lock', 'src/'] },
  user: { title: 'User config', sub: 'optional', items: ['~/.config/idf/config.toml'] },
  idf: {
    title: 'idf',
    sub: 'single Rust binary',
    groups: [
      { name: 'Project', mods: ['manifest', 'project', 'lockfile', 'kconfig'] },
      { name: 'Environment', mods: ['sdk', 'python', 'idf_source', 'tools_json'] },
      { name: 'Build', mods: ['build', 'deps', 'firmware'] },
      { name: 'Device', mods: ['serial', 'monitor', 'image', 'espflash'] },
      { name: 'Thin wrappers', mods: ['size', 'efuse', 'secure', 'coredump', 'debug'] },
    ],
  },
  cache: {
    title: 'Global cache',
    sub: '~/.idf-next/',
    items: ['sdk/<version>/ source · toolchains · venv', 'components/ component downloads', 'downloads/ content-addressed archives'],
  },
  build: {
    title: 'build/<chip>/<profile>/',
    sub: 'generated project and artifacts',
    items: ['project/ generated CMakeLists · main · managed_components', 'sdkconfig · config/', 'firmware .bin · .elf · .map'],
  },
  cmake: { title: 'CMake + Ninja', sub: "IDF's CMake framework, unchanged", items: ['GCC compile and link'] },
  py: {
    title: 'Managed Python environment',
    sub: "the SDK's uv venv",
    items: ['kconfgen · ldgen · build-time scripts', 'component manager', 'esptool · espefuse · espsecure'],
  },
  device: { title: 'Board', sub: 'USB-Serial-JTAG · UART bridge', items: ['flash · monitor · erase', 'OpenOCD + GDB'] },
  edges: {
    read: 'reads',
    install: 'installs / reproduces',
    generate: 'generates',
    configure: 'configure runs PYTHON=venv',
    deps: 'dependency solve → idf.lock',
    flash: 'flash · monitor (native)',
    upstream: 'delegates to upstream',
  },
}

const t = computed(() => (lang.value.startsWith('zh') ? zh : en))
</script>

<template>
  <figure class="arch" role="group" :aria-label="t.caption">
    <!-- inputs -->
    <div class="row inputs">
      <section class="card input">
        <h4>{{ t.project.title }}</h4>
        <p class="sub">{{ t.project.sub }}</p>
        <ul class="mono"><li v-for="i in t.project.items" :key="i">{{ i }}</li></ul>
      </section>
      <section class="card input faint">
        <h4>{{ t.user.title }}</h4>
        <p class="sub">{{ t.user.sub }}</p>
        <ul class="mono"><li v-for="i in t.user.items" :key="i">{{ i }}</li></ul>
      </section>
    </div>

    <div class="edge"><span>↓ {{ t.edges.read }}</span></div>

    <!-- idf -->
    <section class="card idf">
      <header>
        <h3 class="mono">{{ t.idf.title }}</h3>
        <span class="sub">{{ t.idf.sub }}</span>
      </header>
      <div class="groups">
        <div v-for="g in t.idf.groups" :key="g.name" class="group">
          <h5>{{ g.name }}</h5>
          <span v-for="m in g.mods" :key="m" class="chip mono">{{ m }}</span>
        </div>
      </div>
    </section>

    <div class="edges3">
      <div class="edge"><span>↓ {{ t.edges.install }}</span></div>
      <div class="edge"><span>↓ {{ t.edges.generate }}</span></div>
      <div class="edge"><span>↓ {{ t.edges.flash }}</span></div>
    </div>

    <!-- targets -->
    <div class="row targets">
      <section class="card cache">
        <h4>{{ t.cache.title }}</h4>
        <p class="sub mono">{{ t.cache.sub }}</p>
        <ul><li v-for="i in t.cache.items" :key="i">{{ i }}</li></ul>
      </section>
      <section class="card build">
        <h4 class="mono">{{ t.build.title }}</h4>
        <p class="sub">{{ t.build.sub }}</p>
        <ul><li v-for="i in t.build.items" :key="i">{{ i }}</li></ul>
      </section>
      <section class="card device">
        <h4>{{ t.device.title }}</h4>
        <p class="sub">{{ t.device.sub }}</p>
        <ul><li v-for="i in t.device.items" :key="i">{{ i }}</li></ul>
      </section>
    </div>

    <div class="edges3">
      <div class="edge side"><span>{{ t.edges.upstream }} →</span></div>
      <div class="edge"><span>↓ ↑ {{ t.edges.configure }}</span></div>
      <div class="edge side"><span>← {{ t.edges.deps }}</span></div>
    </div>

    <!-- build chain -->
    <div class="row chain">
      <section class="card py">
        <h4>{{ t.py.title }}</h4>
        <p class="sub">{{ t.py.sub }}</p>
        <ul><li v-for="i in t.py.items" :key="i">{{ i }}</li></ul>
      </section>
      <div class="join">⇄</div>
      <section class="card cmake">
        <h4>{{ t.cmake.title }}</h4>
        <p class="sub">{{ t.cmake.sub }}</p>
        <ul><li v-for="i in t.cmake.items" :key="i">{{ i }}</li></ul>
      </section>
    </div>

    <figcaption>{{ t.caption }}</figcaption>
  </figure>
</template>

<style scoped>
.arch {
  --a-border: var(--vp-c-divider);
  --a-bg: var(--vp-c-bg-soft);
  --a-text: var(--vp-c-text-1);
  --a-muted: var(--vp-c-text-2);
  --a-brand: var(--vp-c-brand-1);
  margin: 24px 0;
  padding: 20px;
  border: 1px solid var(--a-border);
  border-radius: 12px;
  background: var(--vp-c-bg);
  color: var(--a-text);
  font-size: 14px;
  line-height: 1.5;
}
.mono { font-family: var(--vp-font-family-mono); }
.row { display: grid; gap: 12px; }
.inputs { grid-template-columns: 2fr 1fr; }
.targets { grid-template-columns: repeat(3, 1fr); }
.chain { grid-template-columns: 1fr auto 1fr; align-items: stretch; }
.join { align-self: center; color: var(--a-muted); font-size: 20px; }

.card {
  border: 1px solid var(--a-border);
  background: var(--a-bg);
  border-radius: 10px;
  padding: 12px 14px;
}
.card h3, .card h4, .card h5 { margin: 0; padding: 0; border: 0; letter-spacing: 0; }
.card h4 { font-size: 14px; font-weight: 600; }
.card .sub { margin: 2px 0 6px; color: var(--a-muted); font-size: 12px; }
.card ul { margin: 0; padding: 0; list-style: none; }
.card li { margin: 2px 0; }
.card li::before { content: '·'; color: var(--a-muted); margin-right: 6px; }
.card ul.mono li { font-size: 13px; }

.input { border-left: 3px solid var(--a-brand); }
.faint { opacity: 0.85; border-left-color: var(--a-border); }

.idf {
  border: 1.5px solid var(--a-brand);
  background: var(--vp-c-brand-soft);
}
.idf header { display: flex; align-items: baseline; gap: 10px; margin-bottom: 10px; }
.idf h3 { font-size: 20px; font-weight: 700; color: var(--a-brand); }
.groups { display: grid; grid-template-columns: repeat(5, 1fr); gap: 10px; }
.group { background: var(--vp-c-bg); border: 1px solid var(--a-border); border-radius: 8px; padding: 8px; }
.group h5 { font-size: 12px; color: var(--a-muted); font-weight: 600; margin-bottom: 6px; }
.chip {
  display: inline-block;
  margin: 0 4px 4px 0;
  padding: 1px 7px;
  border-radius: 999px;
  border: 1px solid var(--a-border);
  background: var(--vp-c-bg-soft);
  font-size: 12px;
}

.edge { text-align: center; color: var(--a-muted); font-size: 12px; padding: 6px 0; }
.edges3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
.edge.side { opacity: 0.9; }

.build { border-top: 3px solid var(--a-brand); }
.cache { border-top: 3px solid var(--vp-c-indigo-1, #6366f1); }
.device { border-top: 3px solid var(--vp-c-green-1, #22a06b); }
.py { border-top: 3px solid var(--vp-c-yellow-1, #d29922); }
.cmake { border-top: 3px solid var(--a-brand); }

figcaption { margin-top: 14px; color: var(--a-muted); font-size: 13px; }

@media (max-width: 800px) {
  .inputs, .targets, .chain { grid-template-columns: 1fr; }
  .groups { grid-template-columns: 1fr 1fr; }
  .edges3 { grid-template-columns: 1fr; }
  .join { transform: rotate(90deg); justify-self: center; }
}
</style>
