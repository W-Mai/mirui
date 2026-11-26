<script setup>
import { computed } from 'vue';
import { useData, withBase } from 'vitepress';
import FrameCutaway from './FrameCutaway.vue';
import ReadingNotice from './ReadingNotice.vue';

const { lang, theme } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const course = computed(() => withBase(zh.value ? '/zh/learn/window.html' : '/learn/window.html'));
const overview = computed(() => withBase(zh.value ? '/zh/learn/' : '/learn/'));

const featured = [
  {
    step: 'I',
    en: 'State & style sheets',
    zh: '状态与样式表',
    descEn: 'Register a Model, read fields inside ${...}, and switch classes without wiring observers by hand.',
    descZh: '把数据放进 Model，在 ${...} 里直读字段；哪一行变了，只有订阅它的节点重算。',
    tag: 'Model · style!',
    href: '/state-management.html',
  },
  {
    step: 'II',
    en: 'Motion & page stacks',
    zh: '运动与页面流转',
    descEn: 'Critical-damped springs and tweens that retarget mid-flight when a user taps again.',
    descZh: '半途打断也能顺滑折返的临界阻尼弹簧与补间，选中框与数值各自走完余程。',
    tag: 'Spring · FollowBounds',
    href: '/navigation.html',
  },
  {
    step: 'III',
    en: 'Shaped text & curves',
    zh: '字形、混排与曲线',
    descEn: 'Bidirectional paragraphs, CJK fallback stacks, text along Bézier paths, and caret geometry.',
    descZh: '双向段落整形、拉丁与中文后备字体栈、沿贝塞尔曲线排字，以及精确到亚像素的光标选区。',
    tag: 'FontToken · Path',
    href: '/typography.html',
  },
  {
    step: 'IV',
    en: 'Backends & bare metal',
    zh: '从 MCU 到浏览器',
    descEn: 'One Q24.8 scene graph across software framebuffers, SDL GPU, WGPU, Web Canvas, DRM, and NuttX.',
    descZh: '同一套 Q24.8 定点几何，既能在 ESP32-C3 的 128×160 小屏上刷局部脏区，也能跑在桌面 GPU 与 WebAssembly。',
    tag: 'no_std · Surface',
    href: '/backend-targets.html',
  },
];

const catalog = [
  {
    idx: '01',
    titleEn: 'Interface & interaction',
    titleZh: '界面与交互',
    items: [
      { en: 'Quickstart and targets', zh: '环境搭建与多端工程', href: '/quickstart.html', code: 'cargo' },
      { en: 'Reactive UI and styles', zh: '响应式状态与样式', href: '/state-management.html', code: 'Model' },
      { en: 'Model collections', zh: '有界列表与键值复用', href: '/model-collections.html', code: 'ModelList' },
      { en: 'Events and callbacks', zh: '手势识别与事件回调', href: '/events.html', code: '#[event]' },
      { en: 'Building custom widgets', zh: '自定义控件与绘制', href: '/widget-events.html', code: '#[view]' },
      { en: 'Navigation and motion', zh: '页面导航与物理动效', href: '/navigation.html', code: '#[animate]' },
      { en: 'Scrolling', zh: '滚动容器与惯性控制', href: '/scrolling.html', code: 'ScrollOffset' },
    ],
  },
  {
    idx: '02',
    titleEn: 'Type, assets & storage',
    titleZh: '排版、资产与持久化',
    items: [
      { en: 'Typography', zh: '字体栈与曲线排字', href: '/typography.html', code: 'FontToken' },
      { en: 'Text editing', zh: '文本编辑与输入法', href: '/text-editing.html', code: 'TextEditing' },
      { en: 'Clipboard', zh: '跨平台剪贴板', href: '/clipboard.html', code: 'Clipboard' },
      { en: 'Resources', zh: '静态与托管资源池', href: '/resources.html', code: 'Handle<T>' },
      { en: 'Shared textures', zh: '跨线程纹理移交', href: '/shared-textures.html', code: 'sync' },
      { en: 'Persistence', zh: '状态快照与自动落盘', href: '/persistence.html', code: '#[persist]' },
    ],
  },
  {
    idx: '03',
    titleEn: 'Backends & diagnostics',
    titleZh: '渲染后端与帧诊断',
    items: [
      { en: 'Surface sizing', zh: '逻辑视口与缩放比', href: '/surface-sizing.html', code: 'Surface' },
      { en: 'Backend targets', zh: '各平台后端实现', href: '/backend-targets.html', code: 'Renderer' },
      { en: 'Backend fallback', zh: '混合后端与软件回退', href: '/backend-fallback.html', code: 'compose_backend!' },
      { en: 'Timing diagnostics', zh: '帧耗时剖析与追踪', href: '/timing-diagnostics.html', code: 'perf' },
      { en: 'CI checks', zh: '编译期契约与门禁', href: '/ci.html', code: 'xtask' },
    ],
  },
];
</script>

<template>
  <main class="manual-home">
    <ReadingNotice />
    <section class="manual-hero">
      <div class="manual-intro">
        <p class="manual-tagline">
          <span class="manual-version">{{ theme.version === 'dev' ? 'dev' : `v${theme.version}` }}</span>
          <i aria-hidden="true">/</i>
          <span>{{ zh ? '这可能是最优雅的声明式嵌入式图形库' : 'Possibly the most elegant declarative embedded graphics library' }}</span>
        </p>
        <h1>
          {{ zh ? '一帧之内，见微知著' : 'See what one frame does' }}<span class="dot-see">.</span>
        </h1>
        <p class="manual-lead">
          {{ zh
            ? '手指落下，Model 翻新，Q24.8 定点布局算出几行脏像素，软件光栅化器或 GPU 把它们画上屏幕。跟着七章动手教程写一个完整的任务清单，从第一行 ui! 写到自动落盘与浏览器运行。'
            : 'A tap lands, a Model flips, Q24.8 layout measures the damaged rows, and the rasterizer repaints only what moved. Build a complete task list across seven runnable chapters.' }}
        </p>
        <div class="manual-actions">
          <a class="site-cta primary" :href="course">
            <span>{{ zh ? '读第一章：打开窗口' : 'Chapter 1: Window' }}</span>
            <span aria-hidden="true">→</span>
          </a>
          <a class="site-cta" :href="overview">{{ zh ? '浏览七章目录' : 'All 7 chapters' }}</a>
          <a class="site-cta subtle" :href="withBase('/quickstart.html')">{{ zh ? '工具链与开发板' : 'Setup & boards' }}</a>
        </div>
      </div>
      <FrameCutaway />
    </section>

    <section class="manual-guides" :aria-label="zh ? '专题指南' : 'Reference guides'">
      <header class="manual-section-head">
        <div>
          <span class="manual-eyebrow">MANUAL · 18 GUIDES</span>
          <h2 id="guides">{{ zh ? '按模块深读' : 'Read by mechanism' }}</h2>
        </div>
        <p class="manual-section-note">{{ zh ? '专题指南暂以英文撰写，附完整可编译的 Rust 片段' : 'Detailed architectural notes and runnable Rust excerpts' }}</p>
      </header>

      <div class="manual-featured-grid">
        <a v-for="guide in featured" :key="guide.href" class="manual-featured-card" :href="withBase(guide.href)">
          <div class="manual-card-top">
            <span class="manual-card-step">{{ guide.step }}</span>
            <code>{{ guide.tag }}</code>
          </div>
          <strong>{{ zh ? guide.zh : guide.en }}</strong>
          <p>{{ zh ? guide.descZh : guide.descEn }}</p>
        </a>
      </div>

      <div class="manual-catalog">
        <div v-for="group in catalog" :key="group.idx" class="manual-catalog-col">
          <h3><span>{{ group.idx }}</span> {{ zh ? group.titleZh : group.titleEn }}</h3>
          <ul>
            <li v-for="item in group.items" :key="item.href">
              <a :href="withBase(item.href)">
                <span>{{ zh ? item.zh : item.en }}</span>
                <code>{{ item.code }}</code>
              </a>
            </li>
          </ul>
        </div>
      </div>
    </section>
  </main>
</template>
