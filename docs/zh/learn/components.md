---
pageClass: tutorial-page
aside: false
---

# 提炼组件与样式表

把重复的筛选按钮收拢成可复用的 `#[compose]` 组件，让已完成的行顺着样式表自动褪色。

<TutorialWorkspace :chapter="4" file="interface.rs">

<<< @/examples/first-app/src/interface.rs#filter-button{rust}

<template #exercise>

勾选或取消几行任务，对比已完成行（柔和灰字）与待办行（深墨色字）的层次差异。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. `#[compose]` 把筛选按钮封装成独立组件：`model!(Tasks)` 负责响应点击切换，`prop!(String)` 接收按钮文字。调用方既可以传静态字符串，也可以传 `${...}` 动态表达式，组件内部只需在最终的 `Button` 上绑定一次。
2. `StyleSheetPlugin` 集中注册了 `task`、`task.complete` 和 `filter` 三条规则。每一行根据 `task.done()` 动态挂载类名；当状态翻转时，只有那一行的文字颜色发生切换，字号等公共排版属性则安静地继承自外层容器。

<<< @/examples/first-app/src/lib.rs#task-styles{rust}

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```

直接写在控件上的内联属性优先级高于样式表类（`class`）。需要随状态切换的外观，统一交给样式表打理最干净。
