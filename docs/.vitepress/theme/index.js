import DefaultTheme from 'vitepress/theme';
import { h } from 'vue';
import VersionBar from './VersionBar.vue';
import ReadingNotice from './ReadingNotice.vue';
import ManualHome from './ManualHome.vue';
import TutorialDemo from './TutorialDemo.vue';
import '../../../gallery/web/site-chrome.css';
import './style.css';

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('ManualHome', ManualHome);
    app.component('TutorialDemo', TutorialDemo);
  },
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-content-after': () => h(VersionBar, { desktop: true }),
      'nav-screen-content-after': () => h(VersionBar),
      'doc-before': () => h(ReadingNotice),
    });
  },
};
