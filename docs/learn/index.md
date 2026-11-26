---
title: Build a task list
description: Start with a bare window and add state bindings, list editing, stylesheets, spring transitions, and local storage, then compile the same crate to the browser.
pageClass: course-page
aside: false
---

<script setup>
import { useData } from 'vitepress';
import CourseOverview from '../.vitepress/theme/CourseOverview.vue';
const { theme } = useData();
</script>

<CourseOverview>

<template #lessons>

## Chapters {#chapters}

1. [**Open a window**<br>Mount a heading and two static rows inside a desktop window.](window.md)
2. [**Complete a task**<br>Tap a row so its checkmark and the header tally flip on the same frame.](state.md)
3. [**Add, remove, and filter**<br>Key rows by `ModelItemId` so filtering and deletion never scramble state.](collections.md)
4. [**Reuse components and styles**<br>Extract a `#[compose]` filter button and dim completed rows via `StyleSheetPlugin`.](components.md)
5. [**Give the interface motion**<br>Tween the progress bar and let a spring-driven outline chase the active filter.](motion.md)
6. [**Save and restore the list**<br>Attach `PersistencePlugin` so tasks survive across restarts.](storage.md)
7. [**Run it in the browser**<br>Hand the exact same `install` function to `WebCanvasSurface` and test it at phone width.](web.md)

</template>

<template #setup>

## Run on your machine {#run}

Install Rust and SDL2, check out the commit matching this manual, and launch chapter 7 natively:

```sh-vue
git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout '{{ theme.source }}'
cargo run -p mirui-first-app --features desktop --example desktop -- 7
```

</template>

</CourseOverview>
