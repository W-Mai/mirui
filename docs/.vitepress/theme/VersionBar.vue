<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useData, useRouter } from 'vitepress';
import { chapterTarget, isRelease, versionBase } from '../lib/versions.mjs';

defineProps({ desktop: Boolean });
const { theme, lang } = useData();
const router = useRouter();
const chinese = computed(() => lang.value === 'zh-CN');
const currentVersion = computed(() => theme.value.version);
const currentLanguage = computed(() => chinese.value ? 'zh' : 'en');
const versions = ref([]);
const selectedVersion = ref('');
const selectedLanguage = ref(currentLanguage.value);
const pending = ref(false);
const error = ref(false);
watch(currentLanguage, language => { selectedLanguage.value = language; });

onMounted(async () => {
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
    const target = chapterTarget(index, page, language, hash);
    if (version === currentVersion.value && !target.includes('?')) await router.go(target);
    else window.location.assign(target);
  } catch {
    error.value = true;
    selectedVersion.value = currentVersion.value;
    selectedLanguage.value = currentLanguage.value;
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <div class="manual-selectors" :class="{ 'manual-selectors-desktop': desktop }">
    <select
      :aria-label="chinese ? '文档版本' : 'Documentation version'"
      :value="selectedVersion || currentVersion"
      :disabled="pending"
      @change="selectedVersion = $event.target.value; navigate(selectedVersion, currentLanguage)"
    >
      <option v-for="version in choices" :key="version" :value="version">
        {{ version === 'dev' ? (chinese ? '开发版' : 'Development') : version }}
      </option>
    </select>
    <select
      :aria-label="chinese ? '文档语言' : 'Documentation language'"
      :value="selectedLanguage"
      :disabled="pending"
      @change="selectedLanguage = $event.target.value; navigate(currentVersion, selectedLanguage)"
    >
      <option value="en">English</option>
      <option value="zh">简体中文</option>
    </select>
    <span v-if="error" role="status">{{ chinese ? '此版本暂不可用' : 'This version is unavailable' }}</span>
  </div>
</template>
