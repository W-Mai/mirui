import DefaultTheme from 'vitepress/theme';
import { h, reactive } from 'vue';
import { workspaceSettings } from './tutorial-runtime';
import VersionBar from './VersionBar.vue';
import ReadingNotice from './ReadingNotice.vue';
import ManualHome from './ManualHome.vue';
import TutorialWorkspace from './TutorialWorkspace.vue';
import SidebarSelection from './SidebarSelection.vue';
import { installReadingMotion } from './reading-motion';
import mark from '../../../gallery/web/favicon.svg?url';
import '../../../gallery/web/site-chrome.css';
import './style.css';
import '../../../gallery/web/device-frame.css';
import './workspace.css';

export default {
  extends: DefaultTheme,
  enhanceApp({ app, router }) {
    app.provide(workspaceSettings, reactive({ mode: 'split', split: 50, width: '', height: '' }));
    if (!import.meta.env.SSR) installReadingMotion(router);
    app.component('ManualHome', ManualHome);
    app.component('TutorialWorkspace', TutorialWorkspace);
  },
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-title-before': () => h('img', { src: mark, alt: '', class: 'manual-mark', width: 28, height: 28 }),
      'nav-bar-content-after': () => h(VersionBar, { desktop: true }),
      'nav-screen-content-after': () => h(VersionBar),
      'doc-before': () => h(ReadingNotice),
      'sidebar-nav-before': () => h(SidebarSelection),
    });
  },
};
