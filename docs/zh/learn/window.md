---
pageClass: tutorial-page
aside: false
---

# 打开窗口

先点亮第一块屏幕：一个深色标题栏，底下排着两行待办事项。

<TutorialWorkspace :chapter="1" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#window{rust}

<template #exercise>

右侧画布里立起了一扇窗口：顶部写着 `Task list`，底下依次是 `Connect the display` 与 `Open a window`。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. 打开教程包的 `examples/desktop.rs`。`SdlSurface` 负责向操作系统申请窗口与采集输入；`App` 装好内置控件与默认系统后，用 `spawn_root()` 造出铺满视口的根节点，再把界面树挂上去。
2. 上面这段 `#[compose]` 函数就是第一屏的全部陈设。外层 `Column` 留出 `20 px` 内边距与 `16 px` 行距，里层色块承托标题，两枚 `Text` 节点紧随其后。窗口标题栏归宿主操作系统管，画布里的 `Text` 才归 `mirui` 排版。试着把第一行文字改成别的，再跑一次看看。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```

如果链接阶段提示找不到 SDL2，先用系统包管理器装好 `libsdl2-dev`（或 macOS 下的 `sdl2`），并确认命令行带上了 `--features desktop`。
