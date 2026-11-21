import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import ArchitectureDiagram from '../components/ArchitectureDiagram.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('ArchitectureDiagram', ArchitectureDiagram)
  },
} satisfies Theme
