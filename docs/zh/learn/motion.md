# 给应用加动效

让完成进度平滑更新，让选中框跟随筛选按钮。

## 顺着代码做 {#source}

1. 从完成数求出归一化的 `Fixed` 目标。进度视图保留显示值和最近的目标。快速反选时，`Transition::retarget` 从当前显示值继续。
2. `#[animate]` 回调把动画推进后的值用于绘制。选中框是一个附了 `FollowBounds` 的普通 `View`，筛选按钮决定目标，`INTERACTIVE` 提供弹簧参数。

<<< @/examples/first-app/src/motion.rs#retarget{rust}

## 运行并核对 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

快速点选再反选，进度条从当前位置反向。切换 Open、Done，选中框滑到对应按钮。

<TutorialDemo :chapter="5" />

完成状态与筛选条件即时更新，进度条和选中框的显示值经过中间状态。
