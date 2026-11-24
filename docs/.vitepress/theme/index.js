import DefaultTheme from 'vitepress/theme';
import { h } from 'vue';
import VersionBar from './VersionBar.vue';
import ReadingNotice from './ReadingNotice.vue';
import '../../../gallery/web/shared-theme.css';
import './style.css';

export default {
  extends: DefaultTheme,
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-content-after': () => h(VersionBar, { desktop: true }),
      'nav-screen-content-after': () => h(VersionBar),
      'doc-before': () => h(ReadingNotice),
    });
  },
};
