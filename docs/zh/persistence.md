# 状态快照与自动落盘 {#persistence}

启用 `persistence` 特性后，即可将 `PersistencePlugin` 与存储后端配合使用，实现模型、信号与资源的自动落盘和启动恢复。

<MiruiSnippet recipe="mini-storage" label="#[persist]" file="recipes.rs" height="200px">

<template #exercise>

连续点击几次 `+ 1 (Autosaves)`，停顿 0.2 秒待状态落盘后，点击底栏右侧的 **`模拟重开 ↻`** 销毁并重建 `App`，验证计数从存档快照中原样恢复。

</template>

</MiruiSnippet>

## 模型字段存档声明 {#model-field-descriptions}

在结构体上标注 `#[model(persistence)]` 即可为其生成字段编解码器（field codec）。为需要写入存档的字段加上 `#[persist]`；`persistence(key = "settings")` 还会同时声明根模型的默认存储键。`#[observe]` 与 `#[persist]` 彼此独立：一个字段可以只观察不落盘，也可以只落盘不观察。内联子模型字段（`model!(owned Child)`）会自动复用子模型自身的编解码器。

```rust
use mirui::model;

#[model(persistence(key = "settings"))]
struct Settings {
    #[persist]
    volume: u8,
    #[observe]
    panel_open: bool,
}

#[model]
impl Settings {}
```

字段编解码器按字段名与存档类型匹配，不依赖声明顺序。缺失或类型不兼容的字段会保留当前内存值，遇到不兼容类型时会输出警告。损坏或非法的记录会在应用到模型之前被整体拒绝。未标注 `#[persist]` 的字段既不会被编码，也不会在恢复时被重置。这些声明不会改变原始模型结构体的内存布局。一旦关闭 `mirui` 的 `persistence` 特性，编解码器实现及其字段特征约束会被一并剥离，应用 crate 无需定义同名特性开关。

## 集合存档 {#collections}

在内联持有的 `ModelList`、`ModelMap` 或 `ModelSet` 字段上标注 `#[persist]`，即可将该集合纳入根模型的存档记录：

```rust
#[model(persistence)]
#[derive(Default)]
struct TodoItem {
    #[persist]
    #[observe]
    done: bool,
}

#[model]
impl TodoItem {}

#[model(persistence(key = "todo"))]
struct Todo {
    #[persist]
    items: model!(owned ModelList<TodoItem>),
}

#[model]
impl Todo {}
```

列表保留元素顺序；映射（Map）按键排序编码业务键与成员；集合（Set）按值排序编码不可变成员。运行时的 `ModelItemId`、观察者列表、预留容量以及副作用处理器不会被写入存档。列表与映射成员类型需要实现 `Default` 并具备字段编解码器；映射的键与集合的值需要实现 `Default`、`Ord` 以及受支持的字段编解码器。新建成员中若存在存档未覆盖的字段，将保留其 `Default` 默认值。

恢复过程会在替换当前数据之前先完整准备好整棵根模型（包括成员的观察存储）。若存档条目数超出集合硬上限，或映射键 / 集合值存在重复，整条存档记录会被拒绝。显式恢复允许分配准备期临时缓冲，但绝不会篡改集合已配置的硬上限。安装阶段本身不再分配框架存储，也不会触发副作用提取器或回调。

从存档恢复集合会退役所有旧成员并分配全新的 `ModelItemId`（即使业务键或值与之前相同）。集合本身的句柄、集合级观察者与集合级副作用处理器保持有效；但原先绑定在已退役旧成员上的回调将停止执行。恢复空的存档集合会清空现有成员并保留已准备的容量。日常的内存重排与 `reconcile` 合并则继续保留匹配成员的稳定身份。

Gallery 中的 [State Todo 示例](../../src/gallery/demos/state_todo/model.rs) 使用默认字段编解码器保存任务标题、完成状态、排列顺序与下一个示例标题下标。启用持久化时，运行时会在注册模型之前先安装存储插件。任务标题是不带 `#[observe]` 的自有字符串，行控件在首次组合时读取一次标题，而完成状态保持可观察；恢复存档时会替换成员身份并根据恢复出的标题重建按键值对齐的行。新增任务会构造业务字符串，而勾选切换与重排不会复制字符串。

## 注册持久化模型 {#registering-a-model}

Gallery 中的 [Reader Settings 示例](../../src/gallery/demos/persistence_settings/model.rs) 存储字号与详情栏开关。控制栏与预览区订阅同一个模型；预览展开状态虽然可观察但并不落盘。其[运行时](../../src/gallery/demos/persistence_settings/runtime.rs)在注册模型前安装存储插件并配置每秒自动保存一次；即使不启用持久化，同一套控件在纯内存模式下依然可以直接使用。

请务必在注册持久化模型**之前**安装 `PersistencePlugin`。`add_model(value)` 会创建一个待决构建器，调用 `.finish()` 时才真正完成注册并返回共享句柄。存档恢复在初始化观察源之前完成。如果直接丢弃构建器而不调用 `.finish()`，则既不会注册模型也不会访问存储（构建器类型带有 `#[must_use]` 提醒）。

```rust
app.add_plugin(PersistencePlugin::new(storage).autosave(1000));

let settings = app.add_model(Settings {
    volume: 60,
    panel_open: false,
}).finish();

let document = app.add_model(Settings {
    volume: 40,
    panel_open: true,
}).with_key("documents/42").finish();
```

声明了 `persistence` 的模型可以调用 `.with_key(...)`，在读取存储之前覆盖类型默认键。存储键必须非空，且在当前所有存活注册项（包含模型、信号、资源与原始字节注册项）中全局唯一。若注册被拒绝（例如键冲突），`finish()` 仍会返回一个可用的内存模型句柄并记录持久化错误日志，不会读取或覆盖冲突的存档。没有存储键的模型不会向存储服务注册。

未安装插件时，模型按普通内存模式运行；事后补装插件不会回扫或恢复此前已注册的实例。若存储中尚无对应记录，模型会保留传入的初始值并标记为待保存候选（save candidate）。若读取失败或存档格式非法，模型同样保留初始值，同时将该条目置于**防覆盖保护（restore protection）**状态，防止自动保存把损坏的旧存档覆盖掉；只有显式执行一次成功的保存操作才会解除该保护。存活的注册表条目仅持有模型的弱引用端点，因此当模型退役释放后，其存储键也会随之释放。

`PersistencePlugin::enabled(false)` 允许在禁用 IO 的状态下注册模型。重新启用持久化后会保留当前内存值并将其标记为待保存，不会倒退回旧存档，也不会清除已有的恢复失败保护。禁用期间调用手动 IO 会返回 `PersistenceError::Disabled`。

带有持久化声明的已注册根模型为整棵内联模型树维护单一的待保存候选状态。任何可变方法调用（即使返回 `Err`）都会在不分配、不拷贝持久化字段的前提下将其标为待保存；只读访问不会修改该标志。未声明持久化的模型使用零大小（ZST）持久化状态。候选追踪与存储注册及写入确认相互独立。

## 自定义模型编解码器 (`ModelCodec`) {#model-codecs}

自定义编解码器配置在 `PersistencePlugin` 上，而非模型声明宏上：

```rust
app.add_plugin(
    PersistencePlugin::new(storage)
        .codec(GameArchive)
        .autosave(1000),
);
let game = app.add_model(GameModel::default()).finish();
```

`GameArchive::Model` 指定目标模型类型。插件在首次读取实例记录前选定编解码器，并在该注册周期内一直沿用。同一类型的多个独立模型实例共享该编解码器；不同 `App` 实例可以配置不同的编解码器。对同一模型类型重复配置编解码器会输出警告并保留第一次的配置。内联子模型始终作为根模型存档的一部分整体读写，不支持单独指定编解码器或存储键。

Gallery 中的 Tidal Atlas 与 Echo Walker 编解码器将回放日志编码进插件的可复用输出缓冲区：`prepare` 负责校验日志并重构临时游戏数据，`apply` 则将准备好的数据安装进模型。由于插件在 `add_model(...).finish()` 之前安装，后续 `App::compose` 构建界面时读取到的直接就是恢复后的游戏状态。

Folding Ark、Twin Beacons 与 Atlas Restoration 同样采用这套流程处理定长游戏存档，在编解码器内完成领域规则校验与校验和比对，业务方法无需自己维护持久化版本号或直接调用存储接口。其中 Atlas Restoration 会在画笔落笔拖拽期间暂缓保存，待一笔完成或取消后再恢复自动落盘。

未配置自定义编解码器时，标注了 `#[persist]` 的字段使用内置字段存档格式。若根模型既未标注任何 `#[persist]` 字段，又未配置自定义编解码器，则不会向存储注册并会输出警告。自定义编解码器可以直接读写原始模型中的任意业务字段（无需标注 `#[persist]`）。

`ModelCodec` 为关联的 `Model` 类型定义完整的字节存档格式：
- `encode` 将数据写入可复用的字节缓冲区；
- `prepare` 在不修改当前模型的前提下校验整条存档记录并准备好待替换的值；
- `apply` 消费准备好的值并原地写入模型（此阶段绝不能再执行堆分配、触发业务副作用或进行可能失败的校验）。

`encode` 返回 `Result<EncodeOutcome, PersistenceError>`：
- 返回 `Encoded` 表示已生成完整记录（包括有意为空的记录）；
- 返回 `Deferred` 表示当前时刻不宜落盘（例如手绘笔触尚未抬起）。暂缓（Deferral）会丢弃已写入的半截字节但保留缓冲区容量，使保存请求继续保持待决，同时原样保留已有的错误状态与防覆盖保护，既不执行底层存储写入也不记作错误。后续的自动保存周期、显式保存请求或应用退出钩子会自动重试，不会额外创建重试定时器。注意：调用 `save` 返回成功仅代表请求已被處理，若需确认数据是否实际写入存储，请检查 `status`。

## 手动读写与状态查询 {#manual-operations}

通过 `app.persistence()` 可按模型句柄手动保存或恢复已注册的根模型。存储键与编解码器已在注册时绑定，调用方无需重复传入：

```rust
app.persistence().save(&settings)?;
app.persistence().restore(&settings)?;
app.persistence().save_all()?;
app.persistence().set_enabled(false)?;
let status = app.persistence().status(&settings);
```

恢复操作会先完整准备好整条记录再一次性应用，随后同步发布所有被观察字段、派生值与具名修订号。静态内联子模型保留各自的身份与订阅；默认编解码器下未标注 `#[persist]` 的字段保留当前值。恢复过程不会触发 `#[effects]` 提取器，也不会重放业务动作。

`PersistenceStatus` 区分以下六种状态：
- `Unavailable`：未安装持久化插件；
- `Unregistered`：该模型未向持久化服务注册；
- `Disabled`：持久化当前已禁用；
- `Clean`：最新候选状态已确认写入存储；
- `Pending`：存在尚未落盘的修改（或上次编码返回了 `Deferred`）；
- `Failed { error, protected }`：最近一次读写失败，并指明是否处于防覆盖保护状态。

状态查询本身不执行存储 IO 或内存分配。普通的模型字段修改不会清除 `Failed` 状态；只有一次成功确认的写入或成功应用的恢复才会将其转为 `Clean`。

## 信号、资源与自定义字节流 {#signals-resources-and-bytes}

除了模型之外，插件构建器还接受 `.signal(key, signal)`、`.resource::<T>(key)` 与 `.bytes(key, save, restore)`。也可以在运行期通过 `app.persistence()`、`App::compose` 内的 `tree.persistence()` 或 `#[compose]` 内的 `res!().persistence()` 动态注册：

```rust,ignore
app.add_resource(NetworkSettings::default());
app.add_plugin(PersistencePlugin::new(storage).autosave(1000));
app.persistence().register_resource::<NetworkSettings>("network")?;
app.persistence().register_signal("counter", counter.clone())?;
app.persistence().register_bytes("cache", save_cache, restore_cache)?;

app.persistence().save(&counter)?;
app.persistence().save("network")?;
```

当 IO 处于启用状态时，注册会立即尝试从已有记录中恢复数据；若记录不存在则保留当前值。恢复资源时即使 `App` 中尚未插入该类型也会直接插入；保存资源时若类型不存在则会跳过。资源与 `Signal` 默认使用 `postcard` 序列化；自定义字节回调则完全自主控制字节格式。

`save(target)` 既可接收模型句柄、`Signal` 实例与 `res!` 资源借用，也可直接接收非模型条目的字符串键。若同一个源被注册到了多个键下，直接按源保存会返回 `AmbiguousTarget`，此时请使用 `save("specific-key")` 指定键名。`remove(key)` 仅删除底层存储中的字节记录，不会注销数据源或重置内存中的当前值。

## 回调中的延迟持久化命令 {#deferred-operations}

在 `#[event]`、`#[system]` 与 `#[timer]` 回调内部，通过 `commands.persistence()` 记录延迟持久化操作。命令会在回调借用全部释放后按顺序执行，并读取执行时刻的最新模型数据，而非回调入队时的快照：

```rust
#[event]
fn save_settings(settings: model!(Settings), commands: commands!()) {
    commands.persistence().save(settings);
}
```

同一个命令接口还提供 `restore(model)`、`save_all()`、`set_enabled(bool)`、`register_resource`、`register_signal`、`register_bytes`、`remove` 与 `autosave`。对于资源参数，无论是只读 `res!(NetworkSettings)` 还是可变 `res!(mut NetworkSettings)`，都统一传入共享借用 `commands.persistence().save(&settings)`：

```rust,ignore
#[event]
fn change_network(settings: res!(mut NetworkSettings), commands: commands!()) {
    settings.server.clear();
    commands.persistence().save(&settings);
}

#[event]
fn save_network(settings: res!(NetworkSettings), commands: commands!()) {
    commands.persistence().save(&settings);
}
```

如需显式预留命令容量，可使用 `CommandCapacity::new().persistence(count)`。

## 自动保存调度 {#automatic-saves}

通过 `autosave(interval)` 配置定期自动保存间隔。整数参数单位为毫秒；也可直接传入 `core::time::Duration` 或 `std::time::Duration`：

```rust
use core::time::Duration;
use mirui::core::{persistence::PersistencePlugin, storage::MemoryStorage};

let plugin = PersistencePlugin::new(MemoryStorage::new()).autosave(1000);
let plugin = plugin.autosave(Duration::from_secs(2));
```

默认情况下定期自动保存处于关闭状态；传入 `0` 可再次关闭。即使未开启定期保存，应用挂起（suspend）与正常退出（quit）生命周期钩子依然会尝试保存所有已注册的待决数据。若要启用基于时间间隔的定期保存，请在安装 `PersistencePlugin` 之前先安装时钟插件（如 `StdInstantClockPlugin`）。到期的自动保存在帧调度的持久化边界处同步执行，不会开启后台定时器线程。

## 存储后端与返回值契约 {#storage-results}

`Storage::read` 返回 `Result<Option<Vec<u8>>, StorageError>`：只有 `Ok(None)` 代表键不存在。`write` 在新旧字节完全相同时返回 `WriteOutcome::Unchanged`，在后端接受写入后返回 `WriteOutcome::Written`。

`FileStorage::open` 与 `LocalStorageStorage::with_prefix` 返回 `Result`。遇到损坏的存档文件时绝不会静默替换为空存储。若文件系统写入或删除失败，内存确认缓存会恢复原状，确保后续重试不会把未落盘的字节误判为已保存。`FileStorage` 在临时文件原子重命名到位后即刻返回，并将磁盘 `fsync` 放到后台线程执行，从而避免阻塞界面帧；当 `FileStorage` 被释放时会等待后台同步完成。每个存储命名空间请保持单一持有者。

`PersistenceError::Capacity` 专门表示准备编码或恢复缓冲区时内存预算不足，与底层存储介质的 IO 错误（`PersistenceError::Storage`）严格区分。自定义字节恢复回调在修改应用状态前应先完整校验记录；保存回调在编码期间切勿提前更新“已保存”标记，因为底层存储仍有可能拒绝该次写入。
