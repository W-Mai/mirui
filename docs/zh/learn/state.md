---
pageClass: tutorial-page
aside: false
---

# 完成一条任务

让任务记住完成状态，并让行状态和完成数同步。

<TutorialWorkspace :chapter="2" file="model.rs">

<<< @/examples/first-app/src/model.rs#task{rust}

<template #exercise>

点第一行后，勾选标记出现，完成数从 1 变成 2。再点一次恢复。

</template>

</TutorialWorkspace>

## 顺着代码做 {#source}

1. 在 compose 前用 `app.add_model(Tasks::default()).finish()` 注册任务清单。每行拿到任务自己的 Model handle，勾选标记和完成数都可以读取 `done`。
2. 行的 `on Tap` 调用 `task.toggle()`，绑定随即更新勾选标记。连续点同一行两次，观察两次反向变化。完成数从任务列表推导出来。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```

在响应式表达式里读生成的 `done()`，让绑定订阅任务字段。
