<script setup>
import { computed } from 'vue';
import { useData, withBase } from 'vitepress';

const { lang, frontmatter } = useData();
const zh = computed(() => lang.value === 'zh-CN');
</script>

<template>
  <div class="course">
    <header class="course-cover">
      <div class="course-intro">
        <span class="course-kicker">{{ zh ? '动手教程 · Rust + WebAssembly' : 'Tutorial · Rust + WebAssembly' }}</span>
        <h1>{{ frontmatter.title }}</h1>
        <p>{{ frontmatter.description }}</p>
        <div class="course-actions">
          <a class="site-cta primary" :href="withBase((zh ? '/zh' : '') + '/learn/window.html')">
            {{ zh ? '从第一章开始' : 'Start chapter one' }} <span aria-hidden="true">→</span>
          </a>
          <a class="site-cta" :href="withBase('/play/?chapter=7')" target="_blank" rel="noopener">{{ zh ? '试用成品' : 'Try the app' }} <span aria-hidden="true">↗</span></a>
        </div>
      </div>
      <div class="course-art" aria-hidden="true">
        <div class="course-pad">
          <div class="course-binding"><i v-for="i in 6" :key="i"></i></div>
          <div class="course-pad-title">Task list</div>
          <div class="course-pad-row"><b></b>Connect the display</div>
          <div class="course-pad-row done"><b>✓</b><s>Open a window</s></div>
          <div class="course-pad-row"><b></b>Try the browser</div>
        </div>
      </div>
    </header>
    <section class="course-lessons"><slot name="lessons" /></section>
    <section class="course-local"><slot name="setup" /></section>
  </div>
</template>

<style src="./course.css"></style>
