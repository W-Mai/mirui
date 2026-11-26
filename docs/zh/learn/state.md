---
pageClass: tutorial-page
aside: false
---

# 点亮第一枚勾选框

让每一行记住自己是否完成，并让顶部的完成计数在同一帧内应声而变。

<TutorialWorkspace :chapter="2" file="model.rs">

<<< @/examples/first-app/src/model.rs#task{rust}

<template #exercise>

点一下第一行 `Connect the display`：勾选框亮起，顶部的计数立刻从 `1 of 3` 跳到 `2 of 3`；再点一下，状态原路退回。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. 在 `compose` 之前调用 `app.add_model(Tasks::default()).finish()`。每一行只拿自己那条 `Task` 的句柄；当 `done` 字段打上 `#[observe]`，谁在 `${...}` 里读了 `task.done()`，谁就自动记入这张订阅表。
2. 行的 `on Tap` 只做一件事——调用 `task.toggle()`。勾选图标的显隐与顶部标题栏里的 `tasks.completed()` 都是从同一份数据源现算出来的，不需要手写回调去两边同步数字，因此永远不会出现“勾上了但计数没变”的错位。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```

注意在 `${...}` 响应式表达式里调用宏生成的 `task.done()` 读取器，而不是直接访问裸字段，这样框架才能精确记录依赖。
