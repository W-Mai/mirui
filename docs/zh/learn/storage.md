---
pageClass: tutorial-page
aside: false
---

# 断电与重开之后

给模型标上持久化字段，哪怕关掉程序再打开，刚才写下的任务与勾选进度依然留在原处。

<TutorialWorkspace :chapter="6" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#persistence{rust}

<template #exercise>

新增或勾选一条任务，停顿半秒让自动落盘完成，然后点击预览框或切换到别章再切回来，看看刚才的改动是不是还在。

</template>

</TutorialWorkspace>

## 拆解这一帧 {#source}

1. `Task` 与 `Tasks` 只给真正需要落盘的业务字段打上 `#[persist]`。在调用 `app.add_model(..).finish()` **之前**先挂好 `PersistencePlugin::new(storage).autosave(500)`，这样模型一注册就会自动从存储介质里还原旧快照，第一帧画出来的就是上次离开时的模样。
2. 桌面端把二进制快照写进 `task-list.mirui` 文件，浏览器端则写入带版本号前缀的 `localStorage`。当前选中的筛选标签和尚未播完的动画属于临时视图状态，故意不落盘；重开之后，进度条直接按恢复出来的完成数就位。

## 在本机运行 {#run}

在当前文档版本对应的仓库根目录下执行：

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```

桌面例程使用当前工作目录下的相对路径存放 `task-list.mirui`，重启验证时保持在同一个目录下运行即可。
