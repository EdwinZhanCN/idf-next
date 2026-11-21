import { defineConfig, type DefaultTheme } from 'vitepress'

// GitHub Pages serves the project at /<repo>/.
const base = process.env.VITEPRESS_BASE ?? '/idf-next/'

const repo = 'https://github.com/EdwinZhanCN/idf-next'

function sidebar(prefix: string, t: {
  guide: string; reference: string; design: string
  items: Record<string, string>
}): DefaultTheme.Sidebar {
  const p = (path: string) => `${prefix}${path}`
  return [
    {
      text: t.guide,
      items: [
        { text: t.items.gettingStarted, link: p('/guide/getting-started') },
        { text: t.items.project, link: p('/guide/project') },
        { text: t.items.sdk, link: p('/guide/sdk') },
        { text: t.items.build, link: p('/guide/build-flash-monitor') },
        { text: t.items.config, link: p('/guide/config') },
        { text: t.items.dependencies, link: p('/guide/dependencies') },
        { text: t.items.debugSecurity, link: p('/guide/debug-security') },
      ],
    },
    {
      text: t.reference,
      items: [
        { text: t.items.commands, link: p('/reference/commands') },
        { text: t.items.idfToml, link: p('/reference/idf-toml') },
        { text: t.items.idfLock, link: p('/reference/idf-lock') },
        { text: t.items.environment, link: p('/reference/environment') },
      ],
    },
    {
      text: t.design,
      items: [
        { text: t.items.architecture, link: p('/design/architecture') },
        { text: t.items.principles, link: p('/design/principles') },
        { text: t.items.status, link: p('/design/status') },
      ],
    },
  ]
}

export default defineConfig({
  base,
  title: 'idf-next',
  cleanUrls: true,
  lastUpdated: true,
  head: [['link', { rel: 'icon', href: `${base}logo.svg` }]],

  themeConfig: {
    logo: '/logo.svg',
    socialLinks: [{ icon: 'github', link: repo }],
    search: { provider: 'local' },
  },

  locales: {
    root: {
      label: '简体中文',
      lang: 'zh-CN',
      description: '用一个 idf.toml 驱动 ESP-IDF 项目的单二进制命令行工具',
      themeConfig: {
        nav: [
          { text: '指南', link: '/guide/getting-started' },
          { text: '参考', link: '/reference/commands' },
          { text: '设计', link: '/design/architecture' },
        ],
        sidebar: sidebar('', {
          guide: '指南', reference: '参考', design: '设计',
          items: {
            gettingStarted: '快速开始', project: '项目与 idf.toml', sdk: 'SDK 与环境',
            build: '构建、烧录与监视', config: '配置', dependencies: '依赖',
            debugSecurity: '调试与安全', commands: '命令', idfToml: 'idf.toml',
            idfLock: 'idf.lock', environment: '环境变量与用户配置',
            architecture: '架构', principles: '设计原则', status: '路线图与现状',
          },
        }),
        outline: { label: '本页内容' },
        docFooter: { prev: '上一页', next: '下一页' },
        lastUpdated: { text: '最后更新于' },
        returnToTopLabel: '回到顶部',
        sidebarMenuLabel: '菜单',
        darkModeSwitchLabel: '外观',
        editLink: {
          pattern: `${repo}/edit/main/site/:path`,
          text: '在 GitHub 上编辑此页',
        },
      },
    },
    en: {
      label: 'English',
      lang: 'en-US',
      link: '/en/',
      description: 'A single-binary CLI that drives ESP-IDF projects from one idf.toml',
      themeConfig: {
        nav: [
          { text: 'Guide', link: '/en/guide/getting-started' },
          { text: 'Reference', link: '/en/reference/commands' },
          { text: 'Design', link: '/en/design/architecture' },
        ],
        sidebar: sidebar('/en', {
          guide: 'Guide', reference: 'Reference', design: 'Design',
          items: {
            gettingStarted: 'Getting started', project: 'Projects and idf.toml',
            sdk: 'SDKs and environment', build: 'Build, flash and monitor',
            config: 'Configuration', dependencies: 'Dependencies',
            debugSecurity: 'Debugging and security', commands: 'Commands',
            idfToml: 'idf.toml', idfLock: 'idf.lock',
            environment: 'Environment and user config',
            architecture: 'Architecture', principles: 'Design principles',
            status: 'Roadmap and status',
          },
        }),
        editLink: {
          pattern: `${repo}/edit/main/site/:path`,
          text: 'Edit this page on GitHub',
        },
      },
    },
  },
})
