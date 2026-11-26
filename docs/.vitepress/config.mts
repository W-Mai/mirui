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
function navigation(zh: boolean) {
  return [
    { text: zh ? '教程' : 'Tutorial', link: zh ? '/zh/learn/' : '/learn/', activeMatch: '/learn/(?!recipes)' },
    { text: zh ? '常用写法' : 'Patterns', link: zh ? '/zh/learn/recipes' : '/learn/recipes', activeMatch: '/learn/recipes' },
    { text: zh ? '指南' : 'Guides', link: zh ? '/zh/#guides' : '/#guides', activeMatch: '^/(?!.*learn/)(?:quickstart|state-management|model-collections|events|widget-events|navigation|scrolling|typography|text-editing|clipboard|resources|shared-textures|persistence|surface-sizing|backend-targets|backend-fallback|timing-diagnostics|ci)\\b' },
    { text: 'Gallery', link: 'https://mirui.rs/' },
    { text: 'API', link: api },
  ];
}
const sidebar = [
  {
    text: 'Start',
    items: [
      { text: 'Build a task list', link: '/learn/' },
      { text: 'Open a window', link: '/learn/window' },
      { text: 'Complete a task', link: '/learn/state' },
      { text: 'Add, remove, and filter', link: '/learn/collections' },
      { text: 'Reuse components and styles', link: '/learn/components' },
      { text: 'Give the interface motion', link: '/learn/motion' },
      { text: 'Save and restore the list', link: '/learn/storage' },
      { text: 'Run it in the browser', link: '/learn/web' },
      { text: 'Common patterns', link: '/learn/recipes' },
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
  appearance: { storageKey: 'mirui-theme' },
  description: 'Build responsive Rust interfaces for small screens, desktop, and the web.',
  srcExclude: ['**/node_modules/**'],
  outDir: process.env.MIRUI_DOCS_OUT_DIR || '.vitepress/dist',
  // VitePress prefetch reads HTML anchor properties on SVG links.
  router: { prefetchLinks: false },
  vite: {
    server: { fs: { allow: [docs, resolve(root, 'gallery/web')] } },
  },
  head: [
    ['link', { rel: 'icon', href: `${base}play/favicon.svg`, type: 'image/svg+xml' }],
    ['script', {}, 'const syncMiruiTheme=()=>{document.documentElement.dataset.theme=document.documentElement.classList.contains("dark")?"dark":"light"};syncMiruiTheme();new MutationObserver(syncMiruiTheme).observe(document.documentElement,{attributes:true,attributeFilter:["class"]});'],
  ],
  locales: {
    root: { label: 'English', lang: 'en' },
    zh: {
      label: '简体中文',
      lang: 'zh-CN',
      themeConfig: {
        nav: navigation(true),
        sidebar: [
          {
            text: '开始上手',
            items: [
              { text: '亲手写一个任务清单', link: '/zh/learn/' },
              { text: '打开窗口', link: '/zh/learn/window' },
              { text: '点亮第一枚勾选框', link: '/zh/learn/state' },
              { text: '增删与筛选列表', link: '/zh/learn/collections' },
              { text: '提炼组件与样式表', link: '/zh/learn/components' },
              { text: '注入物理动效', link: '/zh/learn/motion' },
              { text: '断电与重开之后', link: '/zh/learn/storage' },
              { text: '编译进浏览器画布', link: '/zh/learn/web' },
              { text: '常用界面写法', link: '/zh/learn/recipes' },
              { text: '环境搭建与多端工程', link: '/quickstart' },
            ],
          },
          {
            text: '构建界面',
            items: [
              { text: '响应式状态与样式', link: '/state-management' },
              { text: '有界列表与键值复用', link: '/model-collections' },
              { text: '手势识别与事件回调', link: '/events' },
              { text: '自定义控件与绘制', link: '/widget-events' },
              { text: '页面导航与物理动效', link: '/navigation' },
              { text: '滚动容器与惯性控制', link: '/scrolling' },
            ],
          },
          {
            text: '排版、资产与存储',
            items: [
              { text: '字体栈与曲线排字', link: '/typography' },
              { text: '文本编辑与输入法', link: '/text-editing' },
              { text: '跨平台剪贴板', link: '/clipboard' },
              { text: '静态与托管资源池', link: '/resources' },
              { text: '跨线程纹理移交', link: '/shared-textures' },
              { text: '状态快照与自动落盘', link: '/persistence' },
            ],
          },
          {
            text: '渲染后端与帧诊断',
            items: [
              { text: '逻辑视口与缩放比', link: '/surface-sizing' },
              { text: '各平台后端实现', link: '/backend-targets' },
              { text: '混合后端与软件回退', link: '/backend-fallback' },
              { text: '帧耗时剖析与追踪', link: '/timing-diagnostics' },
              { text: '编译期契约与门禁', link: '/ci' },
            ],
          },
        ],
        outline: { label: '本节内容' },
        docFooter: { prev: '上一节', next: '下一节' },
        editLink: { pattern: `${repository}/blob/${source}/docs/:path`, text: '查看本页源码' },
        skipToContentLabel: '跳到正文',
        returnToTopLabel: '返回顶部',
        sidebarMenuLabel: '文档目录',
      },
    },
  },
  themeConfig: {
    siteTitle: false,
    i18nRouting: false,
    nav: navigation(false),
    socialLinks: [{ icon: 'github', link: repository }],
    sidebar,
    search: {
      provider: 'local',
      options: {
        miniSearch: {
          options: {
            tokenize(text: string) {
              const tokens: string[] = [];
              const cjk = /[\u3400-\u9fff\uf900-\ufaff]/;
              if (typeof Intl !== 'undefined' && Intl.Segmenter) {
                for (const { segment, isWordLike } of new Intl.Segmenter('zh', { granularity: 'word' }).segment(text)) {
                  if (!isWordLike) continue;
                  tokens.push(segment);
                  if (cjk.test(segment) && segment.length > 1) {
                    for (const ch of segment) if (cjk.test(ch)) tokens.push(ch);
                  }
                }
                return tokens;
              }
              return text.toLowerCase().match(/[a-z0-9_]+|[\u3400-\u9fff\uf900-\ufaff]/g) ?? [];
            },
          },
          searchOptions: {
            combineWith: 'AND',
            fuzzy: 0.2,
            prefix: true,
          },
        },
        locales: {
          zh: {
            translations: {
              button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
              modal: {
                noResultsText: '没有找到相关章节',
                displayDetails: '显示匹配段落', resetButtonTitle: '清空搜索', backButtonTitle: '返回',
                footer: { selectText: '打开', navigateText: '选择', closeText: '关闭', selectKeyAriaLabel: '回车', navigateUpKeyAriaLabel: '上方向键', navigateDownKeyAriaLabel: '下方向键', closeKeyAriaLabel: '退出键' },
              },
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
  transformHead({ pageData }) {
    const path = pageData.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '.html');
    if (version === 'dev') return [['meta', { name: 'robots', content: 'noindex' }]];
    return [['link', { rel: 'canonical', href: `https://mirui.rs${base}${path}` }]];
  },
  async buildEnd(config) {
    await indexPages(config.outDir, { version, crateVersion, source });
  },
});
