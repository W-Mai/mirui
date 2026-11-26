---
pageClass: tutorial-page
aside: false
---

# 加上过渡动画

界面里的过渡动画经常会在中途被打断：进度条滑到一半时用户又点了一下，或者在宽度不同的筛选标签之间快速切换。这一章给顶部进度条加上可中途折返的补间（`Tween`），并用弹簧（`Spring`）驱动筛选栏的选中外框。

## 1. 最小演示：`FollowBounds` 弹簧跟随框 {#concept}

先单独看筛选栏的跟随框是怎么工作的。下面这个例子里，三枚按钮宽度各不相同，最后一个 `View` 挂上了 `FollowBounds::new(id("spring_first")).with_transition(...)`。点击任意按钮时，`pick_tab` 事件只需把 `follower.target` 指向被点击的实体，临界阻尼弹簧就会自动算好位置与宽度的过渡：

<MiruiSnippet recipe="mini-motion" label="最小演示" file="recipes.rs" height="220px">

<template #exercise>

在 `MCU`、`Desktop`、`Web` 之间快速来回连点——观察蓝色外框如何在半空中继承当前速度平滑折返，同时自动贴合目标按钮的宽度。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：可折返进度条 + 筛选跟随框 {#app}

现在把这套弹簧外框装进任务清单的筛选栏（见 `interface.rs`），再在 `motion.rs` 里用 `#[system]` + `#[animate]` 给顶部标题栏加一条平滑补间的完成进度条：

<TutorialWorkspace :chapter="5" file="motion.rs">

<template #exercise>

趁顶部进度条还在滑动时快速连点同一行两次，看它如何从半途平滑折返；再点选 `Open` 与 `Done`，观察筛选栏外框的弹簧跟随。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **从当前显示值起跳（Retargeting）**：在 `motion.rs` 的 `aim_progress` 系统里，每当 `goal` 改变，新的补间都是从**当前已经画在屏幕上的 `progress.0`** 出发（`settings.0.motion(progress.0, goal)`），而不是从 0 重新开始。
2. **动画结束即卸载**：`#[animate(component = AnimateProgress)]` 在每一帧更新 `Progress`，一旦抵达终点就会自动从实体上移除 `AnimateProgress` 组件，静止帧零额外开销。

## 在本机运行 {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```
