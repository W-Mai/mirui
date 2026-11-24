<script setup>
import { computed } from 'vue';
import { useData, withBase } from 'vitepress';
import FrameCutaway from './FrameCutaway.vue';
import ReadingNotice from './ReadingNotice.vue';

const { lang, theme } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const course = computed(() => withBase(zh.value ? '/zh/learn/' : '/learn/'));
const guides = [
  { en: 'Reactive state and styles', zh: '响应式状态与样式', href: '/state-management.html' },
  { en: 'Navigation and motion', zh: '导航与动效', href: '/navigation.html' },
  { en: 'Text and typography', zh: '文本与排版', href: '/typography.html' },
  { en: 'Backend targets', zh: '后端与平台', href: '/backend-targets.html' },
];
</script>

<template>
  <main class="manual-home">
    <ReadingNotice hide-development />
    <section class="manual-hero">
      <div class="manual-intro">
        <span class="manual-version">{{ theme.version === 'dev' ? (zh ? '开发版' : 'Development') : theme.version }}</span>
        <h1>{{ zh ? '从一帧开始' : 'Start with a frame.' }}</h1>
        <p>{{ zh ? '用 Mirui 写一个有动效的任务清单。从第一次点击，到桌面和浏览器上的同一份界面。' : 'Build a task list with motion. Follow one interaction all the way from Rust state to the desktop and browser.' }}</p>
        <div class="manual-actions">
          <a class="manual-action" :href="course">{{ zh ? '开始教程' : 'Start the tutorial' }}</a>
          <a :href="withBase('/quickstart.html')">{{ zh ? '安装与平台' : 'Setup and targets' }}</a>
        </div>
      </div>
      <FrameCutaway />
    </section>
    <section class="manual-guides" :aria-label="zh ? '专题指南' : 'Guides'">
      <h2>{{ zh ? '专题指南' : 'Read a guide' }}</h2>
      <a v-for="guide in guides" :key="guide.href" :href="withBase(guide.href)">
        {{ zh ? guide.zh : guide.en }}
        <small v-if="zh">EN</small>
      </a>
    </section>
  </main>
</template>
