---
pageClass: tutorial-page
aside: false
---

# 给应用加动效

让完成进度平滑更新，让选中框跟随筛选按钮。

<TutorialWorkspace :chapter="5" file="motion.rs">

<<< @/examples/first-app/src/motion.rs#progress-motion{rust}

<template #exercise>

快速点选再反选，进度条从当前位置反向。切换 Open、Done，选中框滑到对应按钮。

</template>

</TutorialWorkspace>

## 顺着代码做 {#source}

1. 从完成数求出归一化的 `Fixed` 目标。目标变动时，从当前显示值启动 Tween。动画中再次点击，也从当时显示的位置开始。
2. `#[animate]` 回调更新显示值。动画播完后，动画组件从实体移除。选中框通过 `FollowBounds` 跟随筛选按钮，使用 `INTERACTIVE` 弹簧。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

完成状态与筛选条件即时更新，进度条和选中框的显示值经过中间状态。
