---
title: 动手做一个任务清单
description: 从打开第一个窗口，到勾选、筛选和保存任务。七章共用一个 Rust 工程，每章都能在浏览器里动手。
pageClass: course-page
aside: false
---

<script setup>
import { useData } from 'vitepress';
import CourseOverview from '../../.vitepress/theme/CourseOverview.vue';
const { theme } = useData();
</script>

<CourseOverview>

<template #lessons>

## 课程目录 {#chapters}

1. [**打开窗口**<br>把标题和两行任务画到桌面窗口。](window.md)
2. [**完成一条任务**<br>点一下任务，同时更新勾选状态和完成数。](state.md)
3. [**增删和筛选任务**<br>用输入框添加任务，在全部、待办和已完成之间切换。](collections.md)
4. [**复用组件和样式**<br>让任务行共用样式，完成时自动换色。](components.md)
5. [**给应用加动效**<br>让进度条平滑变化，选中框跟随筛选按钮。](motion.md)
6. [**保存和恢复任务**<br>关闭程序再打开，找回任务和上次的完成状态。](storage.md)
7. [**在浏览器运行并验证**<br>把同一套界面放到 Web Canvas，检查窄屏和桌面效果。](web.md)

</template>

<template #setup>

## 在本机运行 {#run}

安装 Rust 和 SDL2，切换到本手册对应的源码，再运行桌面示例。

```sh-vue
git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout '{{ theme.source }}'
cargo run -p mirui-first-app --features desktop --example desktop -- 7
```

</template>

</CourseOverview>
