---
pageClass: tutorial-page
aside: false
---

# 打开窗口

先看一个只有 30 行的独立小例子，搞懂 `mirui` 是怎么把盒子与文字排进窗口的；然后再把它放进我们要做的任务清单里。

## 1. 最小演示：`Column`、`Row` 与 `Text` {#concept}

在 `mirui` 里，界面是由 `#[compose]` 函数返回的一棵 `ui!` 树。`Column` 垂直排布子节点，`Row` 水平排布子节点，`grow: 1.0` 让容器平分剩余空间。

下面这段代码与右侧画布 **1 : 1 完全对应**（点右上角的 **眼睛按钮** 可以展开完整的 `recipes.rs` 源文件）：

<MiruiSnippet recipe="mini-window" label="最小演示" file="recipes.rs" height="210px">

<template #exercise>

左边写了一个深色横幅和两列并排卡片，右边画布画出的就是这三块区域。拖动右下角圆形手柄试试改变窗口宽度。

</template>

</MiruiSnippet>

---

## 2. 带入任务清单：点亮第一屏 {#app}

掌握了盒子排布，我们正式启动任务清单工程。在第一章里，我们先在顶部放一块深色标题栏 `Task list`，底下排两行静态任务。

注意代码栏里标有 **`+` 号与淡蓝底色**的行——那就是本章写下的核心代码。点击顶部的 **`desktop.rs`** 标签页或关掉 **眼睛按钮**，就能看到 `SdlSurface` 是怎么创建窗口并调用 `install` 的：

<TutorialWorkspace :chapter="1" file="lib.rs">

<template #exercise>

右侧画布里立起了任务清单的第一屏。点一下代码框顶部的 `desktop.rs` 标签，看看原生窗口入口只有短短二十几行。

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
