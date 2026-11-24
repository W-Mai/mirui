import { defineConfig } from 'vitepress';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { indexPages } from './lib/page-index.mjs';
import { versionBase } from './lib/versions.mjs';

const docs = fileURLToPath(new URL('..', import.meta.url));
const root = resolve(docs, '..');
const repository = 'https://github.com/W-Mai/mirui';
const version = process.env.MIRUI_DOCS_VERSION || 'dev';
const base = versionBase(version);
const source = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
const crateVersion = readFileSync(resolve(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)?.[1];
if (!crateVersion || (version !== 'dev' && version !== crateVersion)) {
  throw new Error(`Documentation version ${version} does not match the crate version (${crateVersion}).`);
}
const api = version === 'dev' ? 'https://docs.rs/mirui' : `https://docs.rs/mirui/${version}/mirui/`;
const sidebar = [
  {
    text: 'Start',
    items: [
      { text: 'Quickstart and targets', link: '/quickstart' },
    ],
  },
  {
    text: 'Build interfaces',
    items: [
      { text: 'Reactive UI and styles', link: '/state-management' },
      { text: 'Model collections', link: '/model-collections' },
      { text: 'Events and callbacks', link: '/events' },
      { text: 'Building custom widgets', link: '/widget-events' },
      { text: 'Navigation and motion', link: '/navigation' },
      { text: 'Scrolling', link: '/scrolling' },
    ],
  },
  {
    text: 'Text and assets',
    items: [
      { text: 'Typography', link: '/typography' },
      { text: 'Text editing', link: '/text-editing' },
      { text: 'Clipboard', link: '/clipboard' },
      { text: 'Resources', link: '/resources' },
      { text: 'Shared textures', link: '/shared-textures' },
      { text: 'Persistence', link: '/persistence' },
    ],
  },
  {
    text: 'Backend and development',
    items: [
      { text: 'Surface sizing', link: '/surface-sizing' },
      { text: 'Backend targets', link: '/backend-targets' },
      { text: 'Backend fallback', link: '/backend-fallback' },
      { text: 'Timing diagnostics', link: '/timing-diagnostics' },
      { text: 'CI checks', link: '/ci' },
    ],
  },
];

export default defineConfig({
  base,
  title: 'Mirui',
  description: 'Build responsive Rust interfaces for small screens, desktop, and the web.',
  srcExclude: ['**/node_modules/**'],
  outDir: process.env.MIRUI_DOCS_OUT_DIR || '.vitepress/dist',
  head: [
    ['script', {}, 'const syncMiruiTheme=()=>{document.documentElement.dataset.theme=document.documentElement.classList.contains("dark")?"dark":"light"};syncMiruiTheme();new MutationObserver(syncMiruiTheme).observe(document.documentElement,{attributes:true,attributeFilter:["class"]});'],
  ],
  locales: {
    root: { label: 'English', lang: 'en' },
    zh: {
      label: '简体中文',
      lang: 'zh-CN',
      themeConfig: {
        nav: [{ text: 'Gallery', link: 'https://mirui.rs/#demos' }, { text: 'API 参考', link: api }],
        sidebar: [{ text: '指南', items: [{ text: '文档首页', link: '/zh/' }, { text: '英文快速开始', link: '/quickstart' }] }],
        outline: { label: '本节内容' },
        docFooter: { prev: '上一节', next: '下一节' },
        returnToTopLabel: '返回顶部',
        sidebarMenuLabel: '文档目录',
      },
    },
  },
  themeConfig: {
    siteTitle: 'Mirui Docs',
    i18nRouting: false,
    nav: [{ text: 'Gallery', link: 'https://mirui.rs/#demos' }, { text: 'API Reference', link: api }],
    socialLinks: [{ icon: 'github', link: repository }],
    sidebar,
    search: {
      provider: 'local',
      options: {
        locales: {
          zh: {
            translations: {
              button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
              modal: { noResultsText: '没有找到相关章节' },
            },
          },
        },
      },
    },
    editLink: {
      pattern: `${repository}/blob/${source}/docs/:path`,
      text: 'View source',
    },
    version,
    source,
    api,
  },
  markdown: {
    config(md) {
      md.core.ruler.after('block', 'rustdoc_fences', state => {
        for (const token of state.tokens) {
          if (token.type === 'fence') token.info = token.info.replace(/^rust(?:,[\w-]+)+/, 'rust');
        }
      });
      const renderLink = md.renderer.rules.link_open ||
        ((tokens, index, options, _env, self) => self.renderToken(tokens, index, options));
      md.renderer.rules.link_open = (tokens, index, options, env, self) => {
        const token = tokens[index];
        const href = token.attrGet('href');
        if (href?.startsWith('../') && env.path) {
          const [path, hash] = href.split('#', 2);
          const target = resolve(dirname(env.path), decodeURI(path));
          const entry = relative(root, target);
          const withinRepo = entry && !entry.startsWith('..') && !isAbsolute(entry);
          const outsideDocs = !entry.startsWith('docs/');
          if (withinRepo && outsideDocs && existsSync(target)) {
            const type = statSync(target).isDirectory() ? 'tree' : 'blob';
            token.attrSet('href', `${repository}/${type}/${source}/${entry}${hash ? '#' + hash : ''}`);
          }
        }
        return renderLink(tokens, index, options, env, self);
      };
    },
  },
  sitemap: { hostname: 'https://mirui.rs' },
  async buildEnd(config) {
    await indexPages(config.outDir, { version, crateVersion, source });
  },
});
