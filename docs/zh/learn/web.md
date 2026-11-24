---
pageClass: tutorial-page
aside: false
---

# 在浏览器运行并验证

用 Web Canvas 宿主运行同一套 Model 和界面，再验证各章检查点。

<TutorialWorkspace :chapter="7" file="web.rs">

<<< @/examples/first-app/src/web.rs#web-host{rust}

<template #exercise>

调整预览宽度，添加任务并切换筛选。界面和动效使用与桌面例程相同的 Rust 源码。

</template>

</TutorialWorkspace>

## 顺着代码做 {#source}

1. 桌面和 Web 入口都调用同一个 `install`。Web 创建 `WebCanvasSurface` 并使用 `WebCanvasRendererFactory`。宿主按动画帧调用 `App::tick()`，章节预览关闭后释放它。
2. 运行下面的检查。集成测试挂载各章界面、检查状态更新并重开保存后的任务清单。Playground 构建生成教程实际加载的 Wasm。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```

本地预览若缺 Wasm 产物，在启动文档服务前先执行 playground 构建。
