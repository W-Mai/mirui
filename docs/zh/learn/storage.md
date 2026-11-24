# 保存和恢复任务

关闭并重新运行后，添加过的任务和完成状态还能恢复。

## 顺着代码做 {#source}

1. 在任务和所属列表上声明持久化字段。先安装 `PersistencePlugin`，再调用 `app.add_model(..).finish()`，让数据在首次 compose 前恢复。
2. 桌面例程保存到 `task-list.mirui`，Web 使用带版本的本地存储前缀。筛选与运动的中间值留在界面。进度条直接从恢复后的完成数开始。

<<< @/examples/first-app/src/lib.rs#persistence{rust}

## 运行并核对 {#run}

在当前文档版本对应的仓库根目录执行。

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```

修改一条任务，等半秒自动保存，再关闭并打开应用。

<TutorialDemo :chapter="6" />

桌面例程使用相对路径，重启时保持同一工作目录。
