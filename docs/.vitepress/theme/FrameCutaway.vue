<script setup>
import { computed, onBeforeUnmount, ref } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial } from './tutorial-runtime';
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
  { en: 'Input', zh: '输入与事件', detail: 'Tap', href: '/events.html#recognized-gesture-batches' },
  { en: 'Model and subscriptions', zh: 'Model 与订阅', detail: 'Model', href: '/state-management.html#registered-model-instances' },
  { en: 'Layout and repaint', zh: '布局与局部绘制', detail: 'View · Damage', href: '/backend-fallback.html' },
  { en: 'Rendering and submission', zh: '后端与帧提交', detail: 'Renderer · Surface', href: '/backend-targets.html#renderer-scopes' },
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
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  function draw(now) {
    const elapsed = reduced.matches ? 850 : now - start;
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
  <section class="frame-cutaway" :aria-label="zh ? '一帧的剖面' : 'An exploded view of a frame'">
    <div class="frame-figure" ref="scene" v-html="illustration"></div>
    <div class="frame-details">
      <div class="frame-actions">
        <button class="manual-action" type="button" :disabled="pending" :aria-pressed="checked" @click="toggle">
          {{ zh ? (checked ? '取消勾选任务' : '勾选一条任务') : (checked ? 'Uncheck the task' : 'Complete a task') }}
        </button>
        <span role="status">{{ completed }} / 3 {{ zh ? '完成' : 'complete' }}</span>
      </div>
      <nav class="frame-links" :aria-label="zh ? '架构章节' : 'Architecture chapters'">
        <a v-for="(level, index) in levels" :key="level.href" :href="withBase(level.href)" :class="{ active: active === index }">
          <span>{{ zh ? level.zh : level.en }}</span>
          <code>{{ level.detail }}</code>
        </a>
      </nav>
      <small v-if="failed" role="status">{{ zh ? '示例暂不可用' : 'Example unavailable' }}</small>
    </div>
  </section>
</template>
