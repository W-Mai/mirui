---
pageClass: tutorial-page
aside: false
---

# 增删与筛选列表

当列表需要动态增删和按条件隐藏时，最怕两件事：一是无限增长的堆分配，二是用数组下标当 key 导致删错行或状态串位。

## 1. 最小演示：`ModelList`、`TextInput` 与 `walk` {#concept}

`ModelList::bounded(N)` 提供定长上限的响应式集合，并为每条记录分配一枚终生唯一的 `ModelItemId`。

在下面这个最小演示里，`TextInput` 绑定 `TextDocument` 草稿，点击 `+ Add` 或按回车把文字推入 `ModelList`；`walk ${ notes.items().entries() } with entry by entry.0` 按 `ModelItemId` 渲染每一行：

<MiruiSnippet recipe="mini-list" label="最小演示" file="recipes.rs" height="230px">

<template #exercise>

在输入框里敲几个字按回车（或直接点 `+ Add`），再点任意一行的 `×` 把它删掉。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：新增、删除与三态筛选 {#app}

现在把 `TextInput`、`Remove` 按钮和 `All / Open / Done` 筛选栏装进任务清单。

看左侧聚焦的 `model.rs` 与 `interface.rs`（标 **`+`** 处）：筛选并不会把任务从 `ModelList` 里删掉，而是通过 `visible: ${ tasks.filter().includes(task.done()) }` 控制行的显隐；頂部的完成计数依然统计整份清单：

<ArchFlow kind="uml-ch3-collections" />

<TutorialWorkspace :chapter="3" file="model.rs">

<template #exercise>

在底部输入框添加一条任务，来回切换 `Open` 与 `Done`，再点 `Remove` 删掉某一行，观察其余行的勾选状态是否保持不变。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **按 `ModelItemId` 寻址而非下标**：当你在 `Open` 视图下隐藏了已完成行时，屏幕上第 1 行在底层可能排在第 2 位。因为 `Edit::Remove(key)` 传递的是 `ModelItemId`，无论当前怎么筛选，删除动作永远精准命中目标任务。
2. **输入草稿与业务模型解耦**：正在敲字的光标、选区和缓冲住在独立的 `TextDocument` Model 里，只有真正点击添加或按下回车时，才触发 `edit_tasks` 事件写入 `Tasks`。

## 在本机运行 {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```
