import DefaultTheme from 'vitepress/theme-without-fonts';
import { h, reactive } from 'vue';
import { workspaceSettings } from './tutorial-runtime';
import VersionBar from './VersionBar.vue';
import ReadingNotice from './ReadingNotice.vue';
import ManualHome from './ManualHome.vue';
import TutorialWorkspace from './TutorialWorkspace.vue';
import SidebarSelection from './SidebarSelection.vue';
import { installReadingMotion } from './reading-motion';
import lightMark from '../../assets/logo-light.svg?url';
import darkMark from '../../assets/logo-dark.svg?url';
import '../../../gallery/web/site-chrome.css';
import './style.css';
import '../../../gallery/web/device-frame.css';
import './workspace.css';
import './navigation.css';

export default {
  extends: DefaultTheme,
  enhanceApp({ app, router }) {
    app.provide(workspaceSettings, reactive({ mode: 'split', split: 50, width: '', height: '' }));
    if (!import.meta.env.SSR) installReadingMotion(router);
    app.component('ManualHome', ManualHome);
    app.component('TutorialWorkspace', TutorialWorkspace);
    app.component('MiruiSnippet', TutorialWorkspace);
  },
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-title-before': () => [
        h('img', { class: 'manual-brand light-only', src: lightMark, alt: 'Mirui Docs', width: 77, height: 26 }),
        h('img', { class: 'manual-brand dark-only', src: darkMark, alt: 'Mirui Docs', width: 77, height: 26 }),
        h('span', { class: 'manual-brand-section', 'aria-hidden': 'true' }, 'Docs'),
      ],
      'nav-bar-content-after': () => h(VersionBar),
      'doc-before': () => h(ReadingNotice),
      'doc-footer-before': () => h(ReadingNotice, { revision: true }),
      'sidebar-nav-before': () => h(SidebarSelection),
    });
  },
};
