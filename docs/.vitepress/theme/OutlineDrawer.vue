<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useData, useRoute } from 'vitepress';

const { lang, frontmatter } = useData();
const route = useRoute();
const zh = computed(() => lang.value === 'zh-CN');
const isDocPage = computed(() => frontmatter.value.aside !== false && frontmatter.value.layout !== 'home');

const pinned = ref(false);
const drawerOpen = ref(false);
const headings = ref([]);
const activeId = ref('');
let scrollRaf = 0;

function syncRootClass() {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('outline-pinned', pinned.value);
}

function collectHeadings() {
  if (typeof document === 'undefined' || !isDocPage.value) {
    headings.value = [];
    activeId.value = '';
    return;
  }
  const nodes = document.querySelectorAll('.VPDoc .vp-doc :is(h2, h3)[id]');
  const items = [];
  for (const el of nodes) {
    const clone = el.cloneNode(true);
    for (const anchor of clone.querySelectorAll('.header-anchor')) anchor.remove();
    const title = (clone.textContent || '').trim();
    if (!title) continue;
    items.push({
      id: el.id,
      title,
      level: el.tagName === 'H3' ? 3 : 2,
    });
  }
  headings.value = items;
  updateActiveHeading();
}

function updateActiveHeading() {
  if (!headings.value.length || typeof window === 'undefined') {
    activeId.value = '';
    return;
  }
  const threshold = 116;
  let current = headings.value[0].id;
  for (const item of headings.value) {
    const el = document.getElementById(item.id);
    if (!el) continue;
    const top = el.getBoundingClientRect().top;
    if (top <= threshold) current = item.id;
    else break;
  }
  activeId.value = current;
}

function onScroll() {
  if (scrollRaf) return;
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0;
    updateActiveHeading();
  });
}

function togglePin() {
  pinned.value = !pinned.value;
  if (pinned.value) drawerOpen.value = false;
  try {
    localStorage.setItem('mirui-outline-pinned', pinned.value ? '1' : '0');
  } catch {}
  syncRootClass();
}

function jumpTo(id) {
  activeId.value = id;
  if (!pinned.value) drawerOpen.value = false;
}

const activeItem = computed(() => headings.value.find(h => h.id === activeId.value) || headings.value[0]);
const activeIndex = computed(() => {
  const idx = headings.value.findIndex(h => h.id === activeId.value);
  return idx >= 0 ? idx + 1 : 1;
});

onMounted(() => {
  try {
    pinned.value = localStorage.getItem('mirui-outline-pinned') === '1';
  } catch {}
  syncRootClass();
  nextTick(collectHeadings);
  window.addEventListener('scroll', onScroll, { passive: true });
});

watch(() => [route.path, lang.value], () => {
  drawerOpen.value = false;
  nextTick(() => {
    collectHeadings();
    setTimeout(collectHeadings, 120);
  });
});

onBeforeUnmount(() => {
  if (scrollRaf) cancelAnimationFrame(scrollRaf);
  window.removeEventListener('scroll', onScroll);
});
</script>

<template>
  <div v-if="isDocPage && headings.length > 0" class="outline-controller" :class="{ open: drawerOpen, pinned }">
    <div class="outline-pill-group">
      <button
        type="button"
        class="outline-pill-btn"
        :aria-expanded="drawerOpen || pinned"
        :title="zh ? '展开或收起本页目录' : 'Toggle page table of contents'"
        @click="pinned ? togglePin() : (drawerOpen = !drawerOpen)"
      >
        <span class="outline-pill-iris" aria-hidden="true"></span>
        <span class="outline-pill-kicker">{{ zh ? '本页目录' : 'On this page' }}</span>
        <span class="outline-pill-count">{{ activeIndex }}/{{ headings.length }}</span>
        <span v-if="activeItem" class="outline-pill-current">{{ activeItem.title }}</span>
      </button>
      <button
        type="button"
        class="outline-pin-btn"
        :class="{ active: pinned }"
        :aria-pressed="pinned"
        :title="pinned ? (zh ? '收起右侧常驻目录（释放宽屏空间）' : 'Collapse docked outline for wider reading space') : (zh ? '在右侧固定常驻目录' : 'Dock outline panel on the right')"
        @click="togglePin"
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect x="2" y="2.5" width="12" height="11" rx="2" stroke="currentColor" stroke-width="1.3"/>
          <line x1="10.5" y1="3" x2="10.5" y2="13" stroke="currentColor" stroke-width="1.3"/>
        </svg>
      </button>
    </div>

    <Transition name="outline-pop">
      <nav
        v-if="drawerOpen && !pinned"
        class="outline-floating-panel"
        :aria-label="zh ? '本页小节导航' : 'Page sections'"
        @mouseleave="drawerOpen = false"
      >
        <header class="outline-panel-head">
          <span>{{ zh ? '本页目录' : 'ON THIS PAGE' }}</span>
          <button
            type="button"
            class="outline-dock-action"
            @click="togglePin"
          >{{ zh ? '固定到右侧 ⇥' : 'Dock right ⇥' }}</button>
        </header>
        <ul class="outline-panel-list">
          <li
            v-for="item in headings"
            :key="item.id"
            :class="[`level-${item.level}`, { active: item.id === activeId }]"
          >
            <a :href="`#${item.id}`" @click="jumpTo(item.id)">
              <i v-if="item.id === activeId" class="outline-item-iris" aria-hidden="true"></i>
              <span>{{ item.title }}</span>
            </a>
          </li>
        </ul>
      </nav>
    </Transition>
  </div>
</template>
