<script setup>
import { onMounted, ref, watch } from 'vue';
import { useData, useRoute } from 'vitepress';

const { lang, theme } = useData();
const route = useRoute();
defineProps({ hideDevelopment: Boolean });
const reason = ref('');
const messages = {
  page: ['The selected version has no matching chapter. The nearest section is open.', '所选版本没有对应章节，已打开最近的目录。'],
  anchor: ['This section is unavailable in the selected version. The chapter is open.', '所选版本没有对应小节，已打开这一章。'],
  language: ['This page is available in English for this version.', '该版本暂有英文内容。'],
};
function readReason() {
  reason.value = new URLSearchParams(window.location.search).get('reading');
}
onMounted(readReason);
watch(() => route.path, readReason);
</script>

<template>
  <p v-if="messages[reason]" class="manual-notice" role="status">
    {{ messages[reason][lang === 'zh-CN' ? 1 : 0] }}
  </p>
  <p v-if="theme.version === 'dev' && !hideDevelopment" class="manual-revision">
    {{ lang === 'zh-CN' ? '开发版文档' : 'Development documentation' }}
    · <a :href="`https://github.com/W-Mai/mirui/tree/${theme.source}`"><code>{{ theme.source.slice(0, 8) }}</code></a>
  </p>
</template>
