# 增删和筛选任务

输入并添加任务，按状态筛选，同时保留其余行的身份。

## 顺着代码做 {#source}

1. 用有界的 `ModelList` 保存任务，每个成员都拿到 `ModelItemId`。`walk` 用这个 ID 作为 key，删除时把同一个 ID 交回 Model。
2. 编辑中的标题由 `TextDocument` 保存。Add 读取文本，空输入会生成短标题。筛选绑定行的可见性，顶部完成数仍统计整份清单。

<<< @/examples/first-app/src/model.rs#collection{rust}

## 运行并核对 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```

添加一条有名字的任务，来回切换 Open、Done，再删除新任务。原有任务的状态仍保持。

<TutorialDemo :chapter="3" />

用成员 ID 删除，即使当前有行隐藏或顺序变化，操作仍能命中原任务。
