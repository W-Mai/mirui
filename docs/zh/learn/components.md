# 复用组件和样式

让重复控件共用间距和尺寸，完成的任务行随状态换色。

## 顺着代码做 {#source}

1. 用 `#[compose]` 定义筛选按钮，Model 参数用于业务操作，`prop!(String)` 用于显示文字。调用者可传字面量或动态值，最终在控件上绑定。
2. 用 `StyleSheetPlugin` 注册 `task`、`task.complete`、`filter`。任务行根据状态选 class，每行独立更新。子节点共用容器上的字体和颜色。

<<< @/examples/first-app/src/interface.rs#filter-button{rust}

## 运行并核对 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```

勾选一行，比较它和仍未完成的行在视觉上的区别。

<TutorialDemo :chapter="4" />

控件直接写的字段优先于 class。同一个颜色由 class 控制时，把它集中写在样式表里。
