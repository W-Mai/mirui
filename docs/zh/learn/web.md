---
pageClass: tutorial-page
aside: false
---

# 编译进浏览器画布

前六章我们写的 `model.rs`、`interface.rs`、`motion.rs` 一行都不用改——只需要换一个 20 行的宿主入口，同一个任务清单就能从桌面原生窗口跑进浏览器的 `<canvas>`。

## Web 宿主与桌面宿主并排对照 {#app}

在下面的工作台中，默认聚焦的是 `web.rs` 里的浏览器入口。点一下顶部文件栏里的 **`desktop.rs`** 标签，对比两者的代码：除了把 `SdlSurface` 换成 `WebCanvasSurface`、把 `FileStorage` 换成 `LocalStorageStorage`，中间调用的都是同一个 `install(&mut app, chapter, reduced, storage)`。

你也可以点击右上角的 **眼睛按钮（切换为「全部」）**，随时翻阅这个七章工程全部 6 个源文件的每一行完整代码：

<TutorialWorkspace :chapter="7" file="web.rs">

<template #exercise>

拖动预览框右下角的圆形手柄，把画布收窄成手机竖屏宽度，添加一条任务并切换筛选；点击右下角浮现的尺寸数字即可一键归位。

</template>

</TutorialWorkspace>

### 这一步做了什么 {#source}

1. **同一份 `install`，多端复用**：应用逻辑、响应式模型、样式表和弹簧动画全在 `install` 里，与具体平台彻底解耦。浏览器端只需在 `requestAnimationFrame` 循环中调用 `app.tick()`。
2. **无头集成测试与 Wasm 打包**：在仓库里跑一遍下面两条命令，第一条会在内存无头表面（`App::headless`）上把全部 7 章和所有最小演示挂载、点击、落盘并校验一遍；第二条则用 Trunk 编译出本页右侧正在运行的 WebAssembly 模块。

## 在本机运行与验证 {#run}

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```
