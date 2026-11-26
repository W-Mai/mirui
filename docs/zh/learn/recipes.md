---
pageClass: tutorial-page
aside: false
title: 常用界面写法
---

# 常用界面写法

写界面时最常遇到的四类场景：状态联动与阈值变色、弹性伸缩布局、带阻尼弹簧的选中框跟随，以及自定义矢量绘制控件。

每一段代码都可以在右侧画布里直接点击互动，或拖动右下角手柄检验不同尺寸下的表现。

---

## 状态绑定与阈值换色 {#reactive}

声明一个可观察字段，让数字读数、百分比条宽度与满载告警色在同一帧联动，无需手写同步回调。

<MiruiSnippet recipe="reactive" label="Model" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-reactive{rust}

<template #exercise>

点击 `+ 10` 把数值推到 `80 %` 以上，观察进度条填充色如何跨过阈值自动切换；点 `42` 归位。

</template>

</MiruiSnippet>

`#[observe]` 为 `level` 生成 `gauge.level()` 读取器。`${ Dimension::percent(gauge.level()) }` 与 `${ if gauge.level() >= 80 ... }` 各自订阅同一个字段；调用 `gauge.step(10)` 时，只有受影响的宽度与背景色重新计算。

---

## 弹性宽度与剩余空间分配 {#layout}

左侧主卡片在紧凑与展开宽度之间切换，右侧容器用 `grow: 1.0` 自动填满剩余行宽。

<MiruiSnippet recipe="layout" label="Flexbox" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-layout{rust}

<template #exercise>

点击 `Toggle width` 切换左侧卡片宽度，或拖动右下角手柄拉伸视口，观察右侧容器如何实时伸缩。

</template>

</MiruiSnippet>

固定尺寸（`Dimension::px`）与弹性分配（`grow: 1.0`）写在同一行里。当响应式宽度改变时，布局阶段只重算这一行的几何位置。

---

## 弹簧跟随框 (`FollowBounds`) {#spring}

选中外框独立于按钮之外；在不同宽度的标签之间快速切换时，外框会在半途继承当前速度平滑折返并自适应目标宽度。

<MiruiSnippet recipe="spring" label="FollowBounds" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-spring{rust}

<template #exercise>

在 `MCU`、`Desktop`、`Web` 三个不同宽度的按钮间快速连点，看外框如何在半途平滑折返。

</template>

</MiruiSnippet>

外框本身只是一个不占 Flex 空间的 `View`，挂载 `FollowBounds::new(id("spring_first"))` 后，在 `#[event]` 回调里把 `follower.target` 指向刚被点击的 `entity!()`，弹簧便会自动接管位置与宽高的过渡。

---

## 自定义矢量控件 (`#[view]`) {#vector}

当内置控件不够用时，用 `#[component]` 与 `#[view]` 定义新控件标签，直接向渲染管线提交矢量路径与描边。

<MiruiSnippet recipe="vector" label="#[view]" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-vector{rust}

<template #exercise>

点击任意一枚几何光圈，每次点击都会循环切换它的描边粗细（`2 px → 3 px → 4 px`）与色彩。

</template>

</MiruiSnippet>

`path!(...)` 在编译期构造零堆分配的静态矢量路径；`#[view(component = VectorGlyph)]` 把组件注册成可在 `ui!` 中直接书写的标签，并在绘制阶段把 `100 × 100` 的局部坐标系缩放到控件的实际布局矩形 `paint.rect()`。
