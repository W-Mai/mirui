<script setup>
import { computed, onBeforeUnmount, ref } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial } from './tutorial-runtime';
import { pageMotion } from './reading-motion';
import frameImage from './frame-section.svg?raw';

const { lang } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const scene = ref();
const completed = ref(1);
const checked = ref(false);
const pending = ref(false);
const active = ref(-1);
const failed = ref(false);
const levels = [
  { index: '01', en: 'Hit test', zh: '命中与手势', detail: 'Tap · Gesture', href: '/events.html#recognized-gesture-batches' },
  { index: '02', en: 'Model', zh: 'Model 订阅', detail: 'Task::toggle', href: '/state-management.html#registered-model-instances' },
  { index: '03', en: 'Layout', zh: '布局与脏区', detail: 'View · PrevRect', href: '/backend-fallback.html' },
  { index: '04', en: 'Raster', zh: '光栅与落屏', detail: 'Canvas · Surface', href: '/backend-targets.html#renderer-scopes' },
];
const illustration = frameImage.replace(/href="#chapter-(\d)"/g, (_, layer) => `href="${withBase(levels[layer].href)}"`);
let example;
let frame = 0;
let destroyed = false;
let shown = 1 / 3;

function display(progress) {
  shown = progress;
  scene.value.dataset.done = String(checked.value);
  scene.value.querySelector('[data-meter]').setAttribute('width', String(progress * 64));
}

function setLayer(layer) {
  active.value = layer;
  if (!scene.value) return;
  for (const node of scene.value.querySelectorAll('[data-layer]')) {
    node.dataset.active = String(Number(node.dataset.layer) === layer);
  }
}

function movePulse(point, visible) {
  for (const dot of scene.value.querySelectorAll('[data-pulse], [data-halo]')) {
    dot.setAttribute('cx', String(point.x));
    dot.setAttribute('cy', String(point.y));
    dot.setAttribute('opacity', visible ? '1' : '0');
  }
}

function animate() {
  cancelAnimationFrame(frame);
  const path = scene.value.querySelector('[data-route]');
  const length = path.getTotalLength();
  const from = shown;
  const goal = completed.value / 3;
  const start = performance.now();
  function draw(now) {
    const elapsed = pageMotion.value ? now - start : 850;
    const fraction = Math.min(elapsed / 640, 1);
    const point = path.getPointAtLength(fraction * length);
    const layer = Math.min(3, Math.max(0, Math.round((point.y - 63) / 76)));
    if (active.value !== layer) setLayer(layer);
    movePulse(point, fraction < 1);
    if (layer >= 1) scene.value.dataset.model = String(checked.value);
    if (layer === 3) {
      const delta = Math.max(0, Math.min((elapsed - 510) / 340, 1));
      display(from + (goal - from) * (1 - (1 - delta) ** 3));
    }
    if (elapsed < 850) frame = requestAnimationFrame(draw);
  }
  frame = requestAnimationFrame(draw);
}

async function toggle() {
  if (pending.value) return;
  pending.value = true;
  failed.value = false;
  try {
    const module = await loadTutorial();
    if (destroyed) return;
    example ??= module.FrameExample.create();
    checked.value = example.toggle();
    completed.value = example.completed();
    animate();
  } catch {
    if (!destroyed) failed.value = true;
  } finally {
    if (!destroyed) pending.value = false;
  }
}

onBeforeUnmount(() => {
  destroyed = true;
  cancelAnimationFrame(frame);
  example?.free();
});
</script>

<template>
  <section class="frame-cutaway" :aria-label="zh ? '一帧的四层剖面' : 'Four layers of a single frame'">
    <header class="frame-head">
      <span class="frame-caption">
        <i class="frame-dot" aria-hidden="true"></i>
        {{ zh ? 'FRAME PIPELINE · 单帧剖面' : 'FRAME PIPELINE · 4 PLANES' }}
      </span>
      <span class="frame-readout" role="status">{{ completed }} / 3 {{ zh ? '已勾选' : 'done' }}</span>
    </header>
    <div class="frame-figure" ref="scene" v-html="illustration"></div>
    <div class="frame-details">
      <div class="frame-actions">
        <button class="site-cta primary frame-trigger" type="button" :disabled="pending" :aria-pressed="checked" @click="toggle">
          <span>{{ zh ? (checked ? '撤回这次点击' : '点一下第一行') : (checked ? 'Undo the tap' : 'Tap the first row') }}</span>
        </button>
        <span class="frame-hint">{{ zh ? '信号沿右侧轨迹逐层下传，只重绘脏区像素' : 'Follow the signal down the four planes' }}</span>
      </div>
      <nav class="frame-links" :aria-label="zh ? '单帧四层章节' : 'Frame layers'" @mouseleave="setLayer(-1)">
        <a v-for="(level, index) in levels" :key="level.href" :href="withBase(level.href)"
          :class="{ active: active === index }"
          @mouseenter="setLayer(index)" @focus="setLayer(index)">
          <span class="frame-step">{{ level.index }}</span>
          <span class="frame-label">{{ zh ? level.zh : level.en }}</span>
          <code>{{ level.detail }}</code>
        </a>
      </nav>
      <small v-if="failed" role="status">{{ zh ? 'Wasm 预览未就绪' : 'Preview unavailable' }}</small>
    </div>
  </section>
</template>
