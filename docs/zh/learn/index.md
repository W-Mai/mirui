---
title: 写一个任务清单
description: 从一个空窗口开始，逐步加上状态绑定、列表增删、样式表、弹簧过渡与本地存储，最后编译到浏览器。
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

## 章节目录 {#chapters}

1. [**打开窗口**<br>在桌面窗口里放进标题栏和两行静态任务，跑通第一棵 `ui!` 树。](window.md)
2. [**勾选任务**<br>点击任意一行，让勾选状态和顶部的完成计数在同一帧更新。](state.md)
3. [**增删与筛选列表**<br>接入输入框与 `ModelList`，用 `ModelItemId` 绑定每一行，筛选或删除时不会串行。](collections.md)
4. [**复用组件与样式**<br>用 `#[compose]` 抽离筛选按钮，通过 `StyleSheetPlugin` 让已完成的行自动变暗。](components.md)
5. [**加上过渡动画**<br>给顶部进度条加上可中途折返的补间，让选中框用弹簧跟随当前筛选按钮。](motion.md)
6. [**保存与恢复列表**<br>接入 `PersistencePlugin` 自动保存修改，重开应用后直接恢复之前的列表。](storage.md)
7. [**在浏览器中运行**<br>把同一个 `install` 函数交给 `WebCanvasSurface`，在手机宽度下测试整套交互。](web.md)

</template>

<template #setup>

## 在本机跑起来 {#run}

装好 Rust 与 SDL2，切到当前手册对应的源码版本，直接运行第 7 章的成品窗口：

```sh-vue
git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout '{{ theme.source }}'
cargo run -p mirui-first-app --features desktop --example desktop -- 7
```

</template>

</CourseOverview>
