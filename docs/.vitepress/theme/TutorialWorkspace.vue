<script setup>
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, toRefs, useId, watch } from 'vue';
import { useData, withBase } from 'vitepress';
import { loadTutorial, workspaceSettings } from './tutorial-runtime';
import { buildSourceBundle } from './tutorial-sources';
import { mountPreviewFrame } from '../../examples/first-app/preview-frame.js';

const props = defineProps({
  chapter: { type: Number, default: 0 },
  recipe: { type: String, default: '' },
  label: { type: String, default: '' },
  file: { type: String, required: true },
  height: { type: String, default: '' },
});
const { lang, isDark, theme } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const modes = [['code', 'Code', '代码'], ['split', 'Split', '分栏'], ['preview', 'Preview', '预览']];
const settings = inject(workspaceSettings);
const { mode, split } = toRefs(settings);
const workspace = ref();
const group = useId();
const canvas = ref();
const panes = ref();
const codeBody = ref();
let pointer;
let detachFrame;
const status = ref('loading');
let tutorial;
let generation = 0;
let frame;

const bundle = computed(() => buildSourceBundle({ chapter: props.chapter, recipe: props.recipe }));
const focusMode = ref(true);
const activeTab = ref(0);
const rowTransition = ref('');
const copied = ref(false);
let copyTimer;

const activeFile = computed(() => bundle.value.files[activeTab.value] || bundle.value.files[0]);

const displayedRows = computed(() => {
  if (focusMode.value && bundle.value.focusRows) {
    return bundle.value.focusRows;
  }
  return activeFile.value ? activeFile.value.renderedRows : [];
});

const lineNumWidth = computed(() => {
  const maxLine = displayedRows.value.reduce((m, r) => Math.max(m, r.line || 0), 9);
  return `${String(maxLine).length}ch`;
});

const idealCodeHeight = computed(() => {
  const rows = displayedRows.value.length;
  const px = Math.min(580, Math.max(360, rows * 21.5 + 58));
  return `${Math.round(px)}px`;
});

const hasAdditions = computed(() => displayedRows.value.some(r => r.added));

const pathDisplay = computed(() => {
  if (focusMode.value && bundle.value.focusRows) {
    const codeRows = bundle.value.focusRows.filter(r => !r.gap);
    const paths = [...new Set(codeRows.map(r => r.file.path))];
    const names = [...new Set(codeRows.map(r => r.file.name))];
    return {
      label: names.join(' · '),
      full: paths.join(' · '),
      count: codeRows.length,
      href: `https://github.com/W-Mai/mirui/blob/${theme.value.source || 'main'}/${paths[0]}${paths.length === 1 ? `#L${codeRows[0].line}-L${codeRows[codeRows.length - 1].line}` : ''}`,
    };
  }
  const f = activeFile.value;
  if (!f) return { label: props.file, full: props.file, count: 0, href: 'https://github.com/W-Mai/mirui' };
  return {
    label: f.path,
    full: f.path,
    count: f.lines.length,
    href: `https://github.com/W-Mai/mirui/blob/${theme.value.source || 'main'}/${f.path}`,
  };
});

function visibleRows() {
  const el = codeBody.value;
  if (!el) return [];
  const top = el.scrollTop;
  const rows = [];
  for (const row of el.querySelectorAll('.code-row[data-line]')) {
    if (row.offsetTop >= top + el.clientHeight) break;
    if (row.offsetTop + row.offsetHeight > top) {
      rows.push({
        file: Number(row.dataset.file),
        line: Number(row.dataset.line),
        offset: row.offsetTop - top,
      });
    }
  }
  return rows;
}

async function setFocus(on, targetTab = null) {
  if (!bundle.value.focusRows || (on === focusMode.value && targetTab === null)) return;
  const seen = visibleRows();
  let spot = null;
  if (on) {
    const own = bundle.value.focusRows.filter(r => !r.gap && r.file.index === activeTab.value);
    const next = seen.length && (own.find(r => r.line >= seen[0].line) || own[own.length - 1]);
    const el = codeBody.value;
    const rowEl = next && el?.querySelector(`.code-row[data-line="${next.line}"]`);
    if (rowEl && el) {
      const offset = Math.min(Math.max(rowEl.offsetTop - el.scrollTop, -rowEl.offsetHeight), el.clientHeight / 2);
      spot = { file: activeTab.value, line: next.line, offset };
    }
  } else {
    let target = targetTab;
    if (target === null) {
      const counts = new Map();
      for (const r of seen) counts.set(r.file, (counts.get(r.file) || 0) + 1);
      target = seen.length ? [...counts].sort((a, b) => b[1] - a[1])[0][0] : activeTab.value;
    }
    spot = seen.find(r => r.file === target) || null;
    if (!spot && bundle.value.focusRows) {
      const first = bundle.value.focusRows.find(r => !r.gap && r.file.index === target);
      if (first) spot = { file: target, line: first.line, offset: 48 };
    }
    activeTab.value = target;
  }
  focusMode.value = on;
  rowTransition.value = on ? 'settle' : 'grow';
  await nextTick();
  if (codeBody.value) {
    const rowEl = spot && codeBody.value.querySelector(`.code-row[data-file="${spot.file}"][data-line="${spot.line}"]`);
    codeBody.value.scrollTop = rowEl ? rowEl.offsetTop - spot.offset : 0;
    codeBody.value.scrollLeft = 0;
  }
}

function toggleEye() {
  setFocus(!focusMode.value);
}

function selectTab(index) {
  if (focusMode.value) {
    setFocus(false, index);
  } else {
    activeTab.value = index;
    if (codeBody.value) {
      codeBody.value.scrollTop = 0;
      codeBody.value.scrollLeft = 0;
    }
  }
}

async function copySource() {
  const rows = displayedRows.value.filter(r => !r.gap);
  const text = rows.map(r => r.text).join('\n') + '\n';
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand('copy'); } catch {}
    document.body.removeChild(ta);
  }
  copied.value = true;
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => { copied.value = false; }, 1400);
}

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
    tutorial = props.recipe
      ? module.Tutorial.from_recipe(canvas.value, props.recipe, reduced, isDark.value)
      : new module.Tutorial(canvas.value, props.chapter, reduced, isDark.value, true);
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
watch(() => [props.chapter, props.recipe], () => {
  activeTab.value = 0;
  if (codeBody.value) {
    codeBody.value.scrollTop = 0;
    codeBody.value.scrollLeft = 0;
  }
  start();
}, { flush: 'post' });

onMounted(() => {
  const style = workspace.value.style;
  if (props.height) style.setProperty('--preview-height', props.height);
  else if (settings.width || settings.height) {
    if (settings.width) style.setProperty('--preview-width', settings.width);
    if (settings.height) style.setProperty('--preview-height', settings.height);
  }
  detachFrame = mountPreviewFrame(canvas.value.parentElement, zh.value);
  start();
  document.addEventListener('visibilitychange', resume);
});
onBeforeUnmount(() => {
  clearTimeout(copyTimer);
  if (!props.height) {
    settings.width = workspace.value.style.getPropertyValue('--preview-width');
    settings.height = workspace.value.style.getPropertyValue('--preview-height');
  }
  detachFrame?.();
  release();
  document.removeEventListener('visibilitychange', resume);
});
</script>

<template>
  <section ref="workspace" class="lesson-workspace" :data-mode="mode" :aria-label="zh ? '代码与实时画布' : 'Live code and canvas'">
    <header class="lesson-tools">
      <div class="lesson-meta">
        <span class="lesson-badge">{{ label || (recipe ? (zh ? '最小演示' : 'CONCEPT') : (zh ? `第 ${chapter} / 7 章` : `0${chapter} / 07`)) }}</span>
        <div class="lesson-tabs" role="tablist" :aria-label="zh ? '源文件' : 'Source files'">
          <button
            v-for="f in bundle.files"
            :key="f.path"
            type="button"
            role="tab"
            class="lesson-tab"
            :class="{ 'has-focus': f.focusSet.size > 0, active: !focusMode && activeTab === f.index }"
            :aria-selected="!focusMode && activeTab === f.index"
            @click="selectTab(f.index)"
          >{{ f.name }}</button>
        </div>
      </div>
      <div class="lesson-controls">
        <button
          v-if="bundle.focusRows"
          type="button"
          class="lesson-icon-btn eye-btn"
          :class="{ on: focusMode }"
          :aria-pressed="focusMode"
          :title="focusMode ? (zh ? '正聚焦本章核心代码 — 点击展开完整多文件源码' : 'Showing focused code — click to show full files') : (zh ? '正显示完整文件 — 点击只看本章核心代码' : 'Showing full file — click to focus core code')"
          :aria-label="zh ? '切换核心代码聚焦与完整文件' : 'Toggle focused source'"
          @click="toggleEye"
        >
          <svg v-if="focusMode" width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" stroke="currentColor" stroke-width="1.35"/>
            <circle cx="8" cy="8" r="2.1" fill="var(--logo-see)" stroke="currentColor" stroke-width="1.1"/>
          </svg>
          <svg v-else width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M1.5 8s2.5-4.5 6.5-4.5c1.06 0 2.02.32 2.86.81M14.5 8s-.86 1.55-2.5 2.86M9.9 9.9A2 2 0 0 1 6.1 6.1" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/>
            <path d="M2.5 2.5l11 11" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/>
          </svg>
          <span class="eye-label">{{ focusMode ? (zh ? '核心' : 'Focus') : (zh ? '全部' : 'Full') }}</span>
        </button>
        <button
          type="button"
          class="lesson-icon-btn copy-btn"
          :class="{ copied }"
          :title="zh ? '复制当前展示的代码' : 'Copy shown source'"
          @click="copySource"
        >
          <span>{{ copied ? (zh ? '已复制' : 'Copied') : (zh ? '复制' : 'Copy') }}</span>
        </button>
        <fieldset class="lesson-modes" :style="{ '--selected': modes.findIndex(entry => entry[0] === mode) }" :aria-label="zh ? '工作区视图' : 'Workspace view'">
          <label v-for="[value, en, cn] in modes" :key="value">
            <input v-model="mode" type="radio" :name="group" :value="value" />
            <span>{{ zh ? cn : en }}</span>
          </label>
        </fieldset>
      </div>
    </header>
    <div ref="panes" class="lesson-panes" :style="{ '--split': split + '%', '--code-ideal-height': idealCodeHeight }">
      <div :inert="mode === 'preview'" class="lesson-source">
        <div class="lesson-code-bar">
          <a class="lesson-code-path" :href="pathDisplay.href" target="_blank" rel="noopener" :title="pathDisplay.full">
            <span>{{ pathDisplay.label }}</span>
            <span aria-hidden="true">↗</span>
          </a>
          <span v-if="hasAdditions" class="lesson-diff-hint">
            <i aria-hidden="true">+</i> {{ zh ? '本章新增 / 改动' : 'Added in this step' }}
          </span>
          <span class="lesson-code-count">{{ pathDisplay.count }} {{ zh ? '行' : 'lines' }}</span>
        </div>
        <div ref="codeBody" class="lesson-code-body">
          <div class="code-rows" :class="rowTransition" :style="{ '--ln-w': lineNumWidth }">
            <template v-for="(row, idx) in displayedRows" :key="row.gap ? `gap-${idx}` : `${row.file.index}:${row.line}`">
              <div v-if="row.gap" class="code-row gap" @click="selectTab(row.file.index)">
                <span class="ln">⋯</span>
                <span class="tx"><span class="gap-label">{{ row.file.name }}:{{ row.line }}</span></span>
              </div>
              <div
                v-else
                class="code-row"
                :class="{ mark: row.marked, added: row.added }"
                :data-file="row.file.index"
                :data-line="row.line"
              >
                <span class="ln"><i v-if="row.added" class="add-sign" aria-hidden="true">+</i>{{ row.line }}</span>
                <span class="tx" v-html="row.html"></span>
              </div>
            </template>
          </div>
        </div>
        <div class="lesson-fallback-slot" hidden><slot /></div>
      </div>
      <div :inert="mode !== 'split'" role="separator" tabindex="0" aria-orientation="vertical"
        class="lesson-divider" :aria-label="zh ? '调整分栏宽度' : 'Adjust pane widths'"
        :aria-valuenow="split" aria-valuemin="30" aria-valuemax="70"
        @pointerdown.prevent="grabDivider" @pointermove="moveDivider" @lostpointercapture="endDivider"
        @pointercancel="endDivider" @pointerup="endDivider"
        @keydown.left.prevent="setSplit(split - 2)" @keydown.right.prevent="setSplit(split + 2)"
        @keydown.home.prevent="setSplit(50)" @dblclick="setSplit(50)"></div>
      <div :inert="mode === 'code'" class="lesson-preview" :data-status="status">
        <div class="lesson-device"><div class="preview-frame">
          <canvas ref="canvas" tabindex="0" :aria-label="label || (zh ? '任务清单' : 'Task list')"></canvas>
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
      <div class="lesson-footer-actions">
        <button v-if="recipe === 'mini-storage' || chapter >= 6" type="button" class="lesson-open-ext" @click="start">
          {{ zh ? '模拟重开 ↻' : 'Relaunch ↻' }}
        </button>
        <a class="lesson-open-ext" :href="withBase('/play/?' + (recipe ? 'recipe=' + recipe : 'chapter=' + chapter))" target="_blank" rel="noopener">{{ zh ? '独立窗口' : 'Open separately' }} <span aria-hidden="true">↗</span></a>
      </div>
    </footer>
  </section>
</template>
