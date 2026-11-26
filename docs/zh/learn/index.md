---
title: 亲手写一个任务清单
description: 从一个空窗口写起，逐步加上状态订阅、列表增删、样式表、弹簧动效与自动落盘。七章共用同一个 Rust 工程，每走一步，右侧画布里的程序就多活一分。
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

1. [**打开窗口**<br>把标题栏与两行待办事项排进桌面窗口，跑通第一棵 `ui!` 树。](window.md)
2. [**点亮第一枚勾选框**<br>点击任意一行，让勾选框与顶部完成计数在同一帧里联动。](state.md)
3. [**增删与筛选列表**<br>接入文本输入与 `ModelList`，在切换筛选视图时稳住每一行的身份。](collections.md)
4. [**提炼组件与样式表**<br>用 `#[compose]` 封装按钮，让已完成的行顺着样式表自动褪色。](components.md)
5. [**注入物理动效**<br>给进度条加上中途可折返的补间，让选中框踏着弹簧追随筛选栏。](motion.md)
6. [**断电与重开之后**<br>挂上 `PersistencePlugin`，关掉窗口再回来，刚才勾选的记录分毫未动。](storage.md)
7. [**编译进浏览器画布**<br>同一份代码接上 `WebCanvasSurface`，在任意宽度的视口里检验整套交互。](web.md)

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
