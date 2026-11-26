---
pageClass: tutorial-page
aside: false
---

# 在浏览器中运行

前面写的 `model.rs`、`interface.rs`、`motion.rs` 不需要做任何修改——换一个浏览器宿主入口，同一个任务清单就能从桌面窗口编译进 `<canvas>`。

## Web 宿主与桌面宿主对比 {#app}

点一下顶部文件栏里的 **`desktop.rs`** 标签，对比 `web.rs` 与 `desktop.rs`：除了把 `SdlSurface` 换成 `WebCanvasSurface`、把 `FileStorage` 换成 `LocalStorageStorage`，中间调用的都是同一个 `install(&mut app, chapter, reduced, storage)`：

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
