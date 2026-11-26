<script setup>
import { computed, onMounted, ref } from 'vue';
import { useData, useRouter } from 'vitepress';
import { chapterTarget, isRelease, versionBase } from '../lib/versions.mjs';
import { pageMotion, togglePageMotion } from './reading-motion';

const { theme, lang, isDark } = useData();
const router = useRouter();
const chinese = computed(() => lang.value === 'zh-CN');
const currentVersion = computed(() => theme.value.version);
const currentLanguage = computed(() => chinese.value ? 'zh' : 'en');
const versions = ref([]);
const selectedVersion = ref('');
const pending = ref(false);
const error = ref(false);
const mounted = ref(false);

onMounted(async () => {
  mounted.value = true;
  if (currentVersion.value === 'dev' && /^(?:localhost|127\.0\.0\.1|0\.0\.0\.0|10\.|192\.168\.|172\.(?:1[6-9]|2\d|3[01])\.)/.test(window.location.hostname)) {
    return;
  }
  try {
    const response = await fetch('/docs/versions.json');
    if (!response.ok) return;
    const manifest = await response.json();
    versions.value = manifest.releases.filter(isRelease);
  } catch {
    // An isolated build still serves its own language and source links.
  }
});

const choices = computed(() => [...new Set([currentVersion.value, ...versions.value, 'dev'])]);

async function navigate(version, language) {
  if (pending.value) return;
  const page = window.location.pathname.slice(versionBase(currentVersion.value).length);
  const hash = window.location.hash;
  pending.value = true;
  error.value = false;
  try {
    const response = await fetch(versionBase(version) + 'page-index.json');
    if (!response.ok) throw new Error('Version unavailable');
    const index = await response.json();
    if (index.version !== version) throw new Error('Version index mismatch');
    let target = chapterTarget(index, page, language, hash);
    if (version === currentVersion.value && language === 'zh' && target.includes('?reading=language')
        && new URLSearchParams(window.location.search).get('reading') === 'language') {
      target = versionBase(version) + 'zh/';
    }
    if (language !== currentLanguage.value) {
      try { localStorage.setItem('mirui-language', language); } catch {}
    }
    if (version === currentVersion.value && !target.includes('?')) await router.go(target);
    else window.location.assign(target);
  } catch {
    error.value = true;
    selectedVersion.value = currentVersion.value;
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <div class="manual-tools">
    <label v-if="choices.length > 1" class="manual-version-picker">
      <select class="site-action"
        :aria-label="chinese ? '文档版本' : 'Documentation version'"
        :value="selectedVersion || currentVersion"
        :disabled="pending"
        @change="selectedVersion = $event.target.value; navigate(selectedVersion, currentLanguage)"
      >
        <option v-for="version in choices" :key="version" :value="version">
          {{ version === 'dev' ? (chinese ? '开发版' : 'Development') : version }}
        </option>
      </select>
      <span aria-hidden="true">⌄</span>
    </label>
    <span v-else class="manual-current-version">{{ currentVersion === 'dev' ? (chinese ? '开发版' : 'Development') : currentVersion }}</span>
    <button class="site-action language-toggle" type="button" :disabled="pending"
      :aria-label="chinese ? 'Switch to English' : '切换到中文'" :title="chinese ? 'Switch to English' : '切换到中文'"
      @click="navigate(currentVersion, chinese ? 'en' : 'zh')">{{ chinese ? 'EN' : '中' }}</button>
    <button class="site-action" type="button" :title="chinese ? '切换明暗主题' : 'Toggle color theme'"
      :aria-label="chinese ? '切换明暗主题' : 'Toggle color theme'" @click="isDark = !isDark">
      <span class="vpi-sun dark-only" aria-hidden="true"></span><span class="vpi-moon light-only" aria-hidden="true"></span>
    </button>
    <button class="site-action motion-toggle" type="button" :aria-pressed="mounted ? pageMotion : undefined"
      :title="chinese ? '页面动效' : 'Page motion'" :aria-label="chinese ? '页面动效' : 'Page motion'" @click="togglePageMotion">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 8 Q4 2 7 8 T14.5 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
    </button>
    <span v-if="error" class="manual-nav-error" role="status">{{ chinese ? '此版本暂不可用' : 'This version is unavailable' }}</span>
  </div>
</template>
