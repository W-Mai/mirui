<script setup>
import { computed, nextTick, onBeforeUnmount, ref } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial } from './tutorial-runtime';

const props = defineProps({ chapter: { type: Number, required: true } });
const { lang, isDark } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const canvas = ref();
const running = ref(false);
const pending = ref(false);
const failed = ref(false);
let tutorial;
let nonce = 0;
let frame = 0;

function close() {
  nonce++;
  cancelAnimationFrame(frame);
  tutorial?.free();
  tutorial = undefined;
  pending.value = false;
  running.value = false;
}

async function start() {
  const attempt = ++nonce;
  failed.value = false;
  pending.value = true;
  try {
    const module = await loadTutorial();
    if (attempt !== nonce) return;
    running.value = true;
    await nextTick();
    if (attempt !== nonce) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)');
    tutorial = new module.Tutorial(canvas.value, props.chapter, reduced.matches, isDark.value, true);
    let dark = isDark.value;
    function draw() {
      if (dark !== isDark.value) {
        dark = isDark.value;
        tutorial.set_dark(dark);
      }
      if (tutorial.tick()) close();
      else frame = requestAnimationFrame(draw);
    }
    frame = requestAnimationFrame(draw);
  } catch {
    if (attempt !== nonce) return;
    close();
    failed.value = true;
  } finally {
    if (attempt === nonce) pending.value = false;
  }
}

onBeforeUnmount(close);
</script>

<template>
  <section class="tutorial-demo" :aria-label="zh ? '章节运行示例' : 'Runnable chapter'">
    <div class="tutorial-toolbar">
      <button class="site-cta primary" type="button" :disabled="pending" @click="running ? close() : start()">
        {{ running ? (zh ? '停止示例' : 'Stop example') : pending ? (zh ? '正在加载' : 'Loading') : (zh ? '运行这一章' : 'Run this chapter') }}
      </button>
      <a :href="withBase(`/play/?chapter=${chapter}`)">{{ zh ? '单独打开' : 'Open standalone' }}</a>
    </div>
    <p v-if="failed" role="status">{{ zh ? '示例尚不可用。请用本章的运行命令。' : 'The preview is unavailable. Use the run command above.' }}</p>
    <div class="tutorial-stage">
      <canvas v-if="running" ref="canvas" tabindex="0" :aria-label="zh ? '任务清单画布' : 'Task list canvas'"></canvas>
      <div v-else class="tutorial-placeholder" aria-hidden="true">
        <span></span><span></span><span></span>
      </div>
    </div>
  </section>
</template>
