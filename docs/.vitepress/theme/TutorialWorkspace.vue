<script setup>
import { computed, inject, onBeforeUnmount, onMounted, ref, toRefs, useId, watch } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial, workspaceSettings } from './tutorial-runtime';
import { mountPreviewFrame } from '../../examples/first-app/preview-frame.js';

const props = defineProps({ chapter: { type: Number, required: true }, file: { type: String, required: true } });
const { lang, isDark } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const modes = [['code', 'Code', '代码'], ['split', 'Split', '分栏'], ['preview', 'Preview', '预览']];
const settings = inject(workspaceSettings);
const { mode, split } = toRefs(settings);
const workspace = ref();
const group = useId();
const canvas = ref();
const panes = ref();
let pointer;
let detachFrame;
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

function moveDivider(event) {
  if (pointer !== event.pointerId) return;
  const rect = panes.value.getBoundingClientRect();
  setSplit(100 * (event.clientX - rect.left) / rect.width);
}

function setSplit(percent) {
  split.value = Math.max(30, Math.min(70, Math.round(percent)));
}

function grabDivider(event) {
  if (event.button !== 0 || pointer !== undefined) return;
  pointer = event.pointerId;
  event.currentTarget.setPointerCapture(pointer);
  panes.value.classList.add('dragging');
}

function endDivider(event) {
  if (pointer !== event.pointerId) return;
  pointer = undefined;
  panes.value.classList.remove('dragging');
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
watch(zh, isZh => detachFrame?.setLocale?.(isZh));
watch(() => props.chapter, start, { flush: 'post' });
onMounted(() => {
  const style = workspace.value.style;
  if (settings.width) style.setProperty('--preview-width', settings.width);
  if (settings.height) style.setProperty('--preview-height', settings.height);
  detachFrame = mountPreviewFrame(canvas.value.parentElement, zh.value);
  start();
  document.addEventListener('visibilitychange', resume);
});
onBeforeUnmount(() => {
  settings.width = workspace.value.style.getPropertyValue('--preview-width');
  settings.height = workspace.value.style.getPropertyValue('--preview-height');
  detachFrame?.();
  release();
  document.removeEventListener('visibilitychange', resume);
});
</script>

<template>
  <section ref="workspace" class="lesson-workspace" :data-mode="mode" :aria-label="zh ? '本章代码和预览' : 'Chapter code and preview'">
    <header class="lesson-tools">
      <div class="lesson-meta">
        <span class="lesson-badge">{{ zh ? `第 ${chapter} / 7 章` : `0${chapter} / 07` }}</span>
        <span class="lesson-file">{{ file }}</span>
      </div>
      <fieldset class="lesson-modes" :style="{ '--selected': modes.findIndex(entry => entry[0] === mode) }" :aria-label="zh ? '工作区视图' : 'Workspace view'">
        <label v-for="[value, en, cn] in modes" :key="value">
          <input v-model="mode" type="radio" :name="group" :value="value" />
          <span>{{ zh ? cn : en }}</span>
        </label>
      </fieldset>
    </header>
    <div ref="panes" class="lesson-panes" :style="{ '--split': split + '%' }">
      <div :inert="mode === 'preview'" class="lesson-source"><slot /></div>
      <div :inert="mode !== 'split'" role="separator" tabindex="0" aria-orientation="vertical"
        class="lesson-divider" :aria-label="zh ? '调整分栏宽度' : 'Adjust pane widths'"
        :aria-valuenow="split" aria-valuemin="30" aria-valuemax="70"
        @pointerdown.prevent="grabDivider" @pointermove="moveDivider" @lostpointercapture="endDivider"
        @pointercancel="endDivider" @pointerup="endDivider"
        @keydown.left.prevent="setSplit(split - 2)" @keydown.right.prevent="setSplit(split + 2)"
        @keydown.home.prevent="setSplit(50)" @dblclick="setSplit(50)"></div>
      <div :inert="mode === 'code'" class="lesson-preview" :data-status="status">
        <div class="lesson-device"><div class="preview-frame">
          <canvas ref="canvas" tabindex="0" :aria-label="zh ? '任务清单' : 'Task list'"></canvas>
        </div></div>
        <Transition name="lesson-status"><div v-if="status !== 'ready'" class="lesson-status" role="status">
          <span>{{ status === 'loading' ? (zh ? '正在加载预览…' : 'Loading preview…') : status === 'stopped' ? (zh ? '预览已关闭' : 'Preview closed') : (zh ? '预览加载失败' : 'Preview unavailable') }}</span>
          <button v-if="status !== 'loading'" class="site-cta" @click="start">{{ zh ? '重新打开' : 'Reopen preview' }}</button>
        </div></Transition>
      </div>
    </div>
    <footer class="lesson-footer">
      <div class="lesson-exercise">
        <span class="lesson-try-dot" aria-hidden="true"></span>
        <div><slot name="exercise" /></div>
      </div>
      <a class="lesson-open-ext" :href="withBase('/play/?chapter=' + chapter)" target="_blank" rel="noopener">{{ zh ? '独立窗口' : 'Open separately' }} <span aria-hidden="true">↗</span></a>
    </footer>
  </section>
</template>
