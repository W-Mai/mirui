---
title: Build a task list
description: Go from an empty window to a list you can check off, filter, and save. All seven chapters use one Rust project, with a working app at each step.
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

## Course outline {#chapters}

1. [**Open a window**<br>Draw a title and two tasks in a desktop window.](window.md)
2. [**Complete a task**<br>Update the checkmark and completed count with a tap.](state.md)
3. [**Add, remove, and filter**<br>Enter a task and switch between all, open, and completed items.](collections.md)
4. [**Reuse components and styles**<br>Share row styles and change their color when a task is done.](components.md)
5. [**Give the interface motion**<br>Animate progress and move the selection between filters.](motion.md)
6. [**Save and restore the list**<br>Close the app and recover the tasks you left behind.](storage.md)
7. [**Run it in the browser**<br>Mount the same interface on Web Canvas and check it at different widths.](web.md)

</template>

<template #setup>

## Run locally {#run}

Install Rust and SDL2, check out the source for this manual, then run the desktop example.

```sh-vue
git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout '{{ theme.source }}'
cargo run -p mirui-first-app --features desktop --example desktop -- 7
```

</template>

</CourseOverview>
