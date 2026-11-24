<script setup>
import { computed, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial } from './tutorial-runtime';

const props = defineProps({ chapter: { type: Number, required: true }, file: { type: String, required: true } });
const { lang, isDark } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const modes = [['code', 'Code', '代码'], ['split', 'Split', '分栏'], ['preview', 'Preview', '预览']];
const mode = ref('split');
const group = useId();
const canvas = ref();
const status = ref('loading');
let tutorial;
let generation = 0;
let frame;

function release() {
  generation++;
  cancelAnimationFrame(frame);
  tutorial?.free();
  tutorial = undefined;
}

function draw() {
  frame = undefined;
  if (!tutorial || mode.value === 'code' || document.hidden) return;
  try {
    if (tutorial.tick()) {
      release();
      status.value = 'stopped';
    } else {
      frame = requestAnimationFrame(draw);
    }
  } catch (error) {
    console.error(error);
    release();
    status.value = 'error';
  }
}

function resume() {
  if (frame === undefined) draw();
}

async function start() {
  release();
  const current = generation;
  status.value = 'loading';
  try {
    const module = await loadTutorial();
    if (generation !== current) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    tutorial = new module.Tutorial(canvas.value, props.chapter, reduced, isDark.value, true);
    status.value = 'ready';
    draw();
  } catch (error) {
    if (generation !== current) return;
    console.error(error);
    status.value = 'error';
  }
}

watch(mode, resume, { flush: 'post' });
watch(isDark, dark => tutorial?.set_dark(dark));
watch(() => props.chapter, start, { flush: 'post' });
onMounted(() => {
  start();
  document.addEventListener('visibilitychange', resume);
});
onBeforeUnmount(() => {
  release();
  document.removeEventListener('visibilitychange', resume);
});
</script>

<template>
  <section class="lesson-workspace" :data-mode="mode" :aria-label="zh ? '本章代码和预览' : 'Chapter code and preview'">
    <header class="lesson-tools">
      <span class="lesson-file">{{ file }}</span>
      <fieldset class="lesson-modes" :aria-label="zh ? '工作区视图' : 'Workspace view'">
        <label v-for="[value, en, cn] in modes" :key="value">
          <input v-model="mode" type="radio" :name="group" :value="value" />
          <span>{{ zh ? cn : en }}</span>
        </label>
      </fieldset>
    </header>
    <div class="lesson-panes">
      <div v-show="mode !== 'preview'" class="lesson-source"><slot /></div>
      <div v-show="mode !== 'code'" class="lesson-preview" :data-status="status">
        <canvas ref="canvas" tabindex="0" :aria-label="zh ? '任务清单' : 'Task list'"></canvas>
        <div v-if="status !== 'ready'" class="lesson-status" role="status">
          <span>{{ status === 'loading' ? (zh ? '正在加载预览…' : 'Loading preview…') : status === 'stopped' ? (zh ? '预览已关闭' : 'Preview closed') : (zh ? '预览加载失败' : 'Preview unavailable') }}</span>
          <button v-if="status !== 'loading'" class="site-cta" @click="start">{{ zh ? '重新打开' : 'Reopen preview' }}</button>
        </div>
      </div>
    </div>
    <footer class="lesson-footer">
      <div><slot name="exercise" /></div>
      <a :href="withBase('/play/?chapter=' + chapter)" target="_blank" rel="noopener">{{ zh ? '独立窗口' : 'Open separately' }} ↗</a>
    </footer>
  </section>
</template>
