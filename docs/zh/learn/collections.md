---
pageClass: tutorial-page
aside: false
---

# 增删与筛选列表

敲入一行新任务、按状态筛选视图，或者删掉某一行——其余行的勾选状态与身份纹丝不动。

<TutorialWorkspace :chapter="3" file="model.rs">

<<< @/examples/first-app/src/model.rs#collection{rust}

<template #exercise>

在底部输入框敲几个字（或直接点 `+ Add`），来回切换 `Open` 与 `Done`，再点 `Remove` 删掉刚加的那一行。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. 任务存放在定长上限的 `ModelList` 里（无需无限增长的动态堆分配）。列表为每个成员颁发一枚稳定的 `ModelItemId`；界面里的 `walk ... by row.0` 拿这枚 ID 当键，`Remove` 按钮也把同一枚 ID 传回给 `tasks.remove(key)`。
2. 正在输入的草稿保存在 `TextDocument` 中。点击添加会取走草稿并清空输入框；若输入为空，则自动补上一条 `Task N`。切换筛选时，我们只改变行的 `visible` 绑定，頂部的完成数依然忠实反映整张清单。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```

因为删除操作认的是 `ModelItemId` 而不是行号下标，哪怕当前隐藏了一半任务、或者列表刚刚重排过，也绝不会误删邻居。
