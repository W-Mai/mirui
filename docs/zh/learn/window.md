---
pageClass: tutorial-page
aside: false
---

# 打开窗口

先看一个独立的小例子，了解 `mirui` 是如何把容器与文字排进窗口的；然后再把它放进任务清单里。

## 1. 最小演示：`Column`、`Row` 与 `Text` {#concept}

在 `mirui` 里，界面是由 `#[compose]` 函数返回的一棵 `ui!` 树。`Column` 垂直排布子节点，`Row` 水平排布子节点，`grow: 1.0` 让容器平分剩余空间：

<MiruiSnippet recipe="mini-window" label="最小演示" file="recipes.rs" height="210px">

<template #exercise>

拖动右下角手柄改变视口宽度，观察两列 `grow: 1.0` 卡片如何平分宽度。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：第一版窗口 {#app}

任务清单的第一章在顶部放一块深色标题栏 `Task list`，底下排两行静态任务。点击顶部的 **`desktop.rs`** 标签页或 **眼睛按钮**，可以看到 `SdlSurface` 是如何创建窗口并调用 `install` 的：

<ArchFlow kind="uml-ch1-tree" />

<TutorialWorkspace :chapter="1" file="lib.rs">

<template #exercise>

切到代码框顶部的 `desktop.rs` 标签，查看原生窗口入口是如何把 `SdlSurface` 交给 `install` 的。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **宿主与画布分离**：在 `desktop.rs` 里，`SdlSurface::new("Task list", 420, 440)` 负责向操作系统申请窗口；而在 `lib.rs` 里，`app.spawn_root().id()` 创建铺满视口的根节点，再由 `app.compose(root, first_window())` 挂上界面树。
2. **颜色走语义 Token**：背景和文字没有写死十六进制色值，而是用 `ColorToken::Secondary` 与 `ColorToken::OnSecondary`。切一下页面右上角的明暗主题按钮，画布会自动跟着换色。

## 在本机运行 {#run}

在仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```
