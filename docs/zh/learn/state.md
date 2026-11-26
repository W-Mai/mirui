---
pageClass: tutorial-page
aside: false
---

# 勾选任务

上一章的任务行还只是静态文字。这一章引入响应式状态（`#[model]`），让点击行、切换勾选图标与更新顶部完成数在同一帧完成。

## 1. 最小演示：`#[model]` 与 `${...}` 绑定 {#concept}

先看一个最纯粹的状态开关：结构体打上 `#[model]`，需要被界面观察的字段打上 `#[observe]`。宏会自动生成同名读取方法（如 `state.checked()` 和 `state.taps()`）。

只要在 `ui!` 的 `${...}` 里调用这些读取方法，控件就会自动订阅字段变化；在 `on Tap` 里直接修改模型，画面即刻刷新：

<MiruiSnippet recipe="mini-state" label="最小演示" file="recipes.rs" height="200px">

<template #exercise>

点击右侧按钮：按钮文字、背景色以及顶部的点击次数全部由同一个 `state.flip()` 驱动，没有任何手动 DOM/Widget 赋值。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：行勾选与顶部统计同步 {#app}

现在把同样的机制放进任务清单里。在 `model.rs` 里给 `Task` 加上 `#[observe] pub done: bool`，并在 `Tasks` 上提供派生查询 `completed()`；然后在 `interface.rs` 里把每一行包进 `Button ... on Tap { task.toggle(); }`：

<TutorialWorkspace :chapter="2" file="model.rs">

<template #exercise>

点一下第一行 `Connect the display`：勾选框亮起，顶部的计数立刻从 `1 of 3` 跳到 `2 of 3`；再点一下原路退回。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **单一数据源，派生计数**：顶部的 `format_args!("{} of {} complete", tasks.completed(), tasks.total())` 直接遍历列表统计已完成的任务数。我们不需要维护第二个计数器变量，所以勾选状态与顶部数字永远不会错位。
2. **必须调 getter 方法**：在 `${...}` 里一定要写 `task.done()` 而不是裸字段 `task.done`——只有经过生成的 getter，响应式作用域才能记录下“这个控件依赖这个字段”。

## 在本机运行 {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```
