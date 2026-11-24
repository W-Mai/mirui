---
pageClass: tutorial-page
aside: false
---

# 打开窗口

先跑起一个桌面窗口，里面有标题和两行任务。

<TutorialWorkspace :chapter="1" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#window{rust}

<template #exercise>

窗口里能看到 Task list，以及 Connect the display 和 Open a window 两条任务。

</template>

</TutorialWorkspace>

## 顺着代码做 {#source}

1. 先看教程包的 `examples/desktop.rs`。`SdlSurface` 提供窗口和输入，`App` 注册控件和默认系统，创建根节点，再把界面挂到根节点上。
2. 下面就是第一屏的全部界面代码。修改第一条任务的文字，重新运行这一章。窗口标题由宿主提供，窗口内的 `Text` 才是界面节点。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```

若链接器找不到 SDL2，先安装 SDL2 开发库，并检查是否带了 `desktop` feature。
