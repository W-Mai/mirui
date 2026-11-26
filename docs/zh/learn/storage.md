---
pageClass: tutorial-page
aside: false
---

# 断电与重开之后

任务清单不能一关窗口就失忆。`mirui` 的持久化机制直接长在 `#[model]` 上：标出哪些字段需要落盘，挂上 `PersistencePlugin`，模型在注册的那一刻就会自动找回上次离开时的快照。

## 1. 最小演示：`#[persist]` 与模拟重开 {#concept}

先看一个只有十几行的持久化计数器：`SavedCount` 标上 `#[model(persistence(key = "..."))]`，`value` 字段标上 `#[persist]`。

试着点几下 `+ 1 (Autosaves)`，然后点击工作台右下角的 **`模拟重开 ↻`** 按钮（它会彻底销毁当前 `App` 实例并重新挂载）——看数字是不是完好无损地回来了：

<MiruiSnippet recipe="mini-storage" label="最小演示" file="recipes.rs" height="200px">

<template #exercise>

点几下 `+ 1` 把数字改大，停顿 0.2 秒后点击右下角的 **`模拟重开 ↻`**，验证新建的 `App` 实例是否直接从存储快照恢复了数值。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：自动落盘与启动恢复 {#app}

现在把持久化加进任务清单。在 `model.rs` 里给 `Task` 和 `Tasks` 的业务字段标上 `#[persist]`，并在 `lib.rs` 里确保 **`PersistencePlugin` 在 `app.add_model(...)` 之前安装**：

<TutorialWorkspace :chapter="6" file="model.rs">

<template #exercise>

新增或勾选一条任务，等半秒自动落盘，然后点击右下角的 **`模拟重开 ↻`**（或刷新浏览器页面），看看刚才的改动是不是分毫未动。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **先装插件，再注册模型**：`app.add_model(Tasks::default()).finish()` 内部会立即向已安装的 `PersistencePlugin` 查询是否有旧快照。因此在 `lib.rs` 里必须先把插件挂好，第一帧画出来的才不会闪烁默认值。
2. **只存业务数据，不存临时视图状态**：注意 `Tasks` 里的 `filter` 只标了 `#[observe]` 而没有标 `#[persist]`。上次停在哪个筛选标签、或者滑到一半的动画属于临时视图状态，重开时干净地回到 `Filter::All`，而任务列表与完成数则完整还原。

## 在本机运行 {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```
