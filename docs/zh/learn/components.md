---
pageClass: tutorial-page
aside: false
---

# 复用组件与样式

当同一种按钮或列表行重复出现时，把宽高、圆角和颜色写在每一处会很难维护。`mirui` 用两个机制做复用：**`#[compose]` 组件函数**与 **`StyleSheetPlugin` 样式表**。

## 1. 最小演示：`prop!(T)` 与动态 `class` {#concept}

先看一个只有两枚胶囊按钮的独立例子：
- `StyleSheetPlugin` 注册了公共类 `"pill"`，以及随状态切换的 `"pill.on"` 和 `"pill.off"`；
- `#[compose] fn status_badge(...)` 接收 `prop!(String)` 和 `prop!(bool)`，调用方既可以传静态字面量，也可以传 `${...}` 响应式表达式：

<MiruiSnippet recipe="mini-style" label="最小演示" file="recipes.rs" height="200px">

<template #exercise>

点击任意一枚胶囊按钮，两枚按钮通过各自的动态 `class: ${ ... }` 同步切换 `.pill.on` 与 `.pill.off` 文字配色。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：封装筛选按钮与完成行褪色 {#app}

回到任务清单：我们把 `Open` 和 `Done` 提炼成 `filter_button` 组件，并在 `lib.rs` 里注册 `task`、`task.complete` 和 `filter` 三个样式类。当某一行被勾选时，`interface.rs` 里的 `task_row` 自动挂上 `"task.complete"`，文字随之变为柔和的次要墨色：

<TutorialWorkspace :chapter="4" file="interface.rs">

<template #exercise>

勾选或取消几行任务，观察已完成行如何顺着 `.task.complete` 样式规则自动褪色。点一下代码框顶部的 `lib.rs` 看看样式表是怎么注册的。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **`prop!(T)` 零中间损耗转发**：`filter_button` 的 `title: prop!(String)` 不会在组件入口额外创建一层订阅，而是把读取器直接交给最终的 `Button(text: ${ title })`。
2. **内联属性优先于 `class`**：直接写在控件上的属性优先级高于样式表类。因此想让某个颜色随 `class` 动态切换时，不要在控件上写死内联 `text_color`，统一交给 `StyleSheetPlugin` 管理。

## 在本机运行 {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```
