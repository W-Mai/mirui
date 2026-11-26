<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useData, useRoute, withBase } from 'vitepress';

const { lang, theme } = useData();
const route = useRoute();
defineProps({ revision: Boolean });
const reason = ref('');
const zh = computed(() => lang.value === 'zh-CN');
const messages = {
  page: ['The selected version has no matching chapter. The nearest section is open.', '所选版本没有对应章节，已打开最近的目录。'],
  anchor: ['This section is unavailable in the selected version. The chapter is open.', '所选版本没有对应小节，已打开这一章。'],
  language: ['This guide is currently in English.', '本篇专题指南暂为英文。'],
};
function readReason() {
  reason.value = new URLSearchParams(window.location.search).get('reading');
}
onMounted(readReason);
watch(() => route.path, readReason);
</script>

<template>
  <div v-if="!revision && messages[reason]" class="manual-notice" role="status">
    <span>{{ reason === 'language' ? '本篇专题指南暂为英文 · This guide is in English.' : messages[reason][zh ? 1 : 0] }}</span>
    <span v-if="reason === 'language'" class="manual-notice-links">
      <a :href="withBase('/zh/learn/')">阅读中文教程 →</a>
      <a :href="withBase('/zh/')">返回中文手册首页</a>
    </span>
  </div>
  <p v-if="theme.version === 'dev' && revision" class="manual-revision">
    {{ zh ? '开发版文档' : 'Development documentation' }}
    · <a :href="`https://github.com/W-Mai/mirui/tree/${theme.source}`"><code>{{ theme.source.slice(0, 8) }}</code></a>
    · <a :href="withBase('/llms.txt')" target="_blank" rel="noopener"><code>llms.txt</code></a>
  </p>
</template>
