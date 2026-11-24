<script setup>
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute } from 'vitepress';

const route = useRoute();
const cursor = ref();
let observer;

function align() {
  const el = cursor.value;
  if (!el) return;
  const nav = el.parentElement;
  const active = nav.querySelector('.VPSidebarItem.is-active > .item > .link');
  const appearing = el.hidden;
  el.hidden = !active || !active.getClientRects().length;
  if (el.hidden) return;
  const row = active.getBoundingClientRect();
  const box = nav.getBoundingClientRect();
  if (appearing) el.style.transition = 'none';
  el.style.transform = 'translateY(' + (row.top - box.top + 2) + 'px)';
  el.style.height = Math.max(0, row.height - 4) + 'px';
  if (appearing) {
    el.getBoundingClientRect();
    el.style.removeProperty('transition');
  }
}

onMounted(() => {
  align();
  observer = new ResizeObserver(align);
  observer.observe(cursor.value.parentElement);
});
watch(() => route.path, align, { flush: 'post' });
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <span ref="cursor" class="reading-selection" hidden aria-hidden="true"></span>
</template>
