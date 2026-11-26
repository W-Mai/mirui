---
pageClass: tutorial-page
aside: false
---

# 编译进浏览器画布

把同一份 `install` 函数交给 `WebCanvasSurface`，在任意宽度的视口里检验整套交互与布局自适应。

<TutorialWorkspace :chapter="7" file="web.rs">

<<< @/examples/first-app/src/web.rs#web-host{rust}

<template #exercise>

拖动预览框右下角的圆形手柄，把画布收窄成手机竖屏宽度，试试添加任务与切换筛选；点击右下角浮现的尺寸数字即可一键归位。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. 桌面端与浏览器端共用同一个 `install(&mut app, chapter, reduced, storage)`，一行 UI 代码都不用改。Web 入口只负责创建 `WebCanvasSurface` 与 `WebCanvasRendererFactory`，并在浏览器的 `requestAnimationFrame` 循环里推进 `app.tick()`；当教程工作台切走或关闭时，及时释放实例。
2. 在仓库里跑一遍下面两条命令：集成测试会在无头表面上挂载全部七章界面、模拟点击增删并验证快照恢复；`playground` 脚本则用 Trunk 把这套 Rust 工程编译成你在本页右侧直接把玩的 WebAssembly 模块。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```

如果在本地起文档站时发现右侧画布未加载，先跑一次 `bun run --cwd docs playground` 生成最新的 Wasm 产物即可。
