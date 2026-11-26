---
pageClass: tutorial-page
aside: false
---

# 注入物理动效

进度条不再僵硬跳变，筛选栏的选中外框踏着临界阻尼弹簧滑向新目标。

<TutorialWorkspace :chapter="5" file="motion.rs">

<<< @/examples/first-app/src/motion.rs#progress-motion{rust}

<template #exercise>

趁进度条还在滑动时快速连点同一行两次，看它如何从半途平滑折返；再点选 `Open` 与 `Done`，观察蓝色外框的跟随轨迹。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. `aim_progress` 系统在每帧动画阶段根据完成数与总数算出归一化的 `Fixed` 目标比值。一旦目标值变了，就以**当前画面上已经滑到的位置**（`progress.0`）为起点发起一段新的 `Tween`。所以哪怕在半途反悔连点，进度条也绝不会闪烁或瞬移回起点。
2. `#[animate]` 回调在每一帧把插值结果写回 `Progress` 组件，抵达终点后自动卸下动画组件，后续帧零开销。筛选栏外框则直接给 `View` 挂上 `FollowBounds`，用 `INTERACTIVE` 弹簧自动追随当前激活按钮的几何边界。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

底层 `Model` 在点击瞬间就已经改完；只有屏幕上的进度宽度和跟随框在时间轴上平滑过渡。
