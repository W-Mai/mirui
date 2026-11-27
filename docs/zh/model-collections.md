# 有界列表与键值复用 {#model-collections}

`ModelList<T>` 存储有序的模型序列；`ModelMap<K, T>` 将排好序的业务键关联到对应的模型实例；`ModelSet<K>` 追踪不可变值的成员资格。三者均位于 `mirui::core::model` 下，可通过 `model!(owned ...)` 声明为父模型的内联字段。只需注册一次父模型，集合内部的所有增删改查都会共享同一套事务与观察生命周期。

<MiruiSnippet recipe="mini-list" label="ModelList" file="recipes.rs" height="230px">

<template #exercise>

在输入框里键入文字并按回车（或点击 `+ Add`）向有界 `ModelList` 追加条目，再点击任意行右侧的 `×` 按稳定 `ModelItemId` 将其移除。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="有界模型集合、稳定 ModelItemId 身份句柄与 RetainedItems 按键行复用流水线">
  <svg viewBox="0 0 880 176" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="mc-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="198" height="132" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · COLLECTION</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">有界模型集合存储</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ModelList&lt;T&gt; · ModelMap&lt;K,V&gt;</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ModelSet&lt;T&gt; · bounded(N)</text>
    <text x="32" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">容量上限内增删零重分配</text>
    <path d="M214 88H252" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#mc-arrow)" />
    <rect x="260" y="22" width="196" height="132" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="276" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · IDENTITY</text>
    <text x="276" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">稳定 ModelItemId</text>
    <text x="276" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">entries() → (id, item)</text>
    <text x="276" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">get_by_id(id) · reconcile</text>
    <text x="276" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">重排与归并全程保留身份</text>
    <path d="M456 88H494" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#mc-arrow)" />
    <rect x="502" y="22" width="196" height="132" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="518" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · RECONCILE</text>
    <text x="518" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">RetainedItems 键值协调</text>
    <text x="518" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">walk $entries by entry.0</text>
    <text x="518" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">LazyList(items: ...)</text>
    <text x="518" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">按 ModelItemId 精准对齐行</text>
    <path d="M698 88H736" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#mc-arrow)" />
    <rect x="744" y="22" width="120" height="132" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="760" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · RETAINED</text>
    <text x="760" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">兄弟节点原地复用</text>
    <text x="760" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">保留焦点与动效</text>
    <text x="760" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">零销毁重建</text>
    <text x="760" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">状态与绑定不丢失</text>
  </svg>
</div>

## 键值关联数据 (`ModelMap`) {#keyed-data}

```rust
use mirui::{model, app::App};
use mirui::core::model::{CollectionError, ModelMap};

#[model]
struct Task {
    #[observe]
    done: bool,
}

#[model]
impl Task {
    fn toggle(&mut self) {
        self.done = !self.done;
    }
}

#[model]
struct Tasks {
    items: model!(owned ModelMap<u64, Task>),
}

#[model]
impl Tasks {
    #[observe]
    fn remaining(&self) -> usize {
        self.items.values().filter(|item| !item.done()).count()
    }
}

fn main() -> Result<(), CollectionError> {
    let data = ModelMap::from_entries([
        (7, Task { done: false }),
        (2, Task { done: false }),
    ]).map_err(|(error, _input)| error)?;
    let mut app = App::headless(32, 32);
    let tasks = app.add_model(Tasks { items: data.into() }).finish();
    tasks.items().get(&7).unwrap().toggle();
    assert_eq!(tasks.remaining(), 1);
    Ok(())
}
```

`from_entries` 接收任意 `(key, data)` 迭代器（包括 `BTreeMap` 或 `HashMap`），直接移动输入数据、按键排序，并在遇到重复键时返回 `(CollectionError::DuplicateKey, input)`——返回的 `input` 按键顺序完整保留所有原始条目，全程不要求键或模型实现 `Clone`。

`get` 与 `contains_key` 支持借用查询（例如 `ModelMap<String, Item>` 可直接用 `&str` 查询）。`try_insert(key, data)` 返回 `(ModelItemId, Option<Item>)`：新键返回 `None`，已存在的键则返回被替换的旧数据并**保留原有的 `ModelItemId` 身份**；若因容量不足等原因失败，则原样返还 `(error, key, data)`。

## 遍历与条目身份 {#iteration-and-identity}

| 操作 | 已注册集合的返回结果 |
| --- | --- |
| List `get(index)` | 当前下标位置的可选成员模型句柄（`Option<ItemHandle>`） |
| Map `get(&key)` | 对应业务键的可选成员模型句柄 |
| `get_by_id(id)` | 对应集合身份（`ModelItemId`）的可选成员模型句柄 |
| List `iter()` / Map `values()` | 按列表顺序或键序排列的成员句柄迭代器 |
| `entries()` | 按相同顺序排列的 `(ModelItemId, item)` 二元组迭代器 |
| Map `for_each(callback)` | 对每个条目借用业务键与成员句柄 |

已注册集合的 `entries`、`iter` 与 `values` 在迭代器被释放前会持有一份**结构租约（structural lease）**：遍历期间依然可以调用单个成员模型的方法，但如果尝试增删集合本身则会返回 `CollectionError::Iterating`；任何可能整体替换该集合的祖先模型方法也会在执行前被拒绝。Map 的 `for_each` 在回调执行期间借用内部键，因此回调内不能修改同一根模型树中的任何模型。未注册的原始 `ModelMap` 的 `iter` 与 `iter_mut` 则遵循常规的 Rust 借用规则返回 `(key, item)` 引用。

**位置下标与业务键都不是条目身份**。删除一个条目会使此前指向它的所有句柄失效；哪怕随后重新插入相同的业务键，旧句柄也不会复活。相反，列表重排（reorder）与映射的同键替换会保留原有的 `ModelItemId`。当已捕获某个成员句柄的事件回调、系统或定时器触发时，若该成员已被退役删除，框架会自动跳过执行。

## 虚拟化行列表 (`LazyList`) {#virtualized-rows}

将已注册的集合传给 `LazyList.items` 即可启用虚拟化列表（类型会自动转换）。复用上文的 `Tasks` 模型：

```rust,ignore
use mirui::prelude::*;
use mirui::ui::widgets::{LazyList, ListItem, Text};
use mirui::ui::widgets::lazy_list::CreateCell;

#[compose]
fn task_cell(cell: model!(ListItem), tasks: model!(Tasks)) -> Entity {
    ui! {
        Text(text: ${
            cell.id()
                .and_then(|id| tasks.items().get_by_id(id))
                .map(|task| if task.done() { "Done" } else { "Open" })
                .unwrap_or("")
        }) on Tap {
            if let Some(task) = cell.id().and_then(|id| tasks.items().get_by_id(id)) {
                task.toggle();
            }
        }
    }
}

#[compose]
fn task_list(tasks: model!(Tasks)) -> Entity {
    ui! {
        LazyList(items: tasks.items(), height: 240, item_extent: 40)
        on CreateCell(|event: event!(CreateCell)| {
            ui! { task_cell(event.item.clone(), tasks.clone()) }
        })
    }
}
```

其中 `cell: model!(ListItem)` 代表**可复用的视觉行槽位**，而 `tasks` 代表底层数据集。请务必在每个响应式属性与事件回调内部实时读取 `cell.id()` 再去集合中查找——如果在创建单元格时只把某个任务捕获一次，那么当该视觉行滚动复用到其他条目时，它仍然会指向旧任务。`ModelList` 与 `ModelMap` 共用完全相同的单元格生命周期（Map 按键序展示）。关于按数量虚拟化、数据源替换与输入取消，请参阅[虚拟化单元格](scrolling.md#virtualized-cells)。

## 成员副作用汇聚 (`on_effect`) {#member-effects}

列表与映射通过 `on_effect` 将其直接成员声明的 `#[effects]` 汇聚向上暴露。回调接收 `&ModelItemEffect<E>` 借用，内含触发该副作用的成员稳定 `id`（`ModelItemId`）以及原始事件载荷 `event`：

```rust
use mirui::{app::App, model};
use mirui::core::model::{EffectRegistrationError, ModelItemEffect, ModelList};

struct Note(u8);

#[model]
struct Pad {
    pending: Option<Note>,
}

#[model]
impl Pad {
    fn strike(&mut self, pitch: u8) {
        self.pending = Some(Note(pitch));
    }

    #[effects]
    fn take_note(&mut self) -> Option<Note> {
        self.pending.take()
    }
}

fn main() -> Result<(), EffectRegistrationError> {
    let mut app = App::headless(32, 32);
    let pads = app.add_model(ModelList::from([Pad { pending: None }])).finish();
    let (id, pad) = pads.entries().next().unwrap();
    pads.on_effect(move |effect: &ModelItemEffect<Note>| {
        assert_eq!(effect.id, id);
        assert_eq!(effect.event.0, 60);
    })?;
    pad.strike(60);
    Ok(())
}
```

每条副作用事件在所有模型观察值发布完毕后，先送达该成员自身的独立消费者，再送达集合级消费者；两者借用同一份事件载荷，因此 `E` 既不需要实现 `Copy` 也不需要实现 `Clone`。列表按最终排列顺序派发，映射按键序派发。注意：如果同时在成员和集合上安装了同一种副作用的消费者，两边都会执行，因此请将音频播放等外部动作统一挂在应当拥有它的那一层。

每个集合对每种事件类型接受一个消费者。列表重排、成员替换、清空、批量加载（`load`）与归并（`reconcile`）都会保留集合级消费者；被删除的成员会失去其独立消费者，后续复用的槽位不会继承旧成员的消费者。集合内部只维护一张与成员容量无关的类型路由表，派发时直接借用载荷，零克隆、零按成员转发分配。

## 绑定动态行 (`walk ... by`) {#binding-rows}

将 `entries()` 与带 `by` 子句的 `walk` 配合使用，可以在集合顺序改变时让每一行控件紧紧跟随它所对应的成员模型：

```rust,ignore
#[compose]
fn task_rows(tasks: model!(Tasks)) {
    ui! {
        Column {
            walk ${ tasks.items().entries() } with entry by entry.0 {
                task_row(entry.1)
            }
        }
    };
}
```

`task_row` 是接收 `item: model!(Task)` 的子组合函数，其内部的响应式属性与事件回调均绑定到该成员实例。重排集合会完整保留行实体及其绑定；删除成员只销毁对应行；插入新成员则构造新行。纯重排操作不产生任何临时表分配。

## 容量规划与内存预算 {#capacity}

`new()` 创建容量为 0 的空集合；`with_capacity(n)` 预先分配容量；`bounded(n)` 在预分配的同时设定不可逾越的硬上限。`reserve(additional)` 准备至少 `len() + additional` 个槽位（含已注册成员的观察与事务元数据）。删除元素会保留已分配容量。日常的插入与替换不会触发扩容分配；只有显式构造、批量导入或调用 `reserve` 时才会分配内存。

嵌套集合的内部形状（shape）根据初始安装的数据准备。僅有外层空闲槽位并不足以容纳更大的内层嵌套集合：当内层预备槽位不足时，普通插入或替换会返回 `CollectionError::UnpreparedShape`。

此外，观察存储的开销还取决于已安装的消费者数量：在每个响应式图分区内，依赖位图占用 `source_capacity * ceil(consumer_capacity / 64) * 8` 字节。`ModelMap` 查找采用二分搜索，按键有序存储；在容量上限内的编辑与已注册成员更新全程复用已准备好的框架内存。

## 批量加载列表与映射 (`load`) {#loading-list-and-map-data}

`items.load(records)` 接收模型数据的自有迭代器，一次性替换数据集并**退役此前的所有成员身份（`ModelItemId`）**。集合字段本身、已分配容量、硬上限以及集合级观察者保持不变。`load` 是显式允许分配的操作，可以为更大的嵌套集合准备存储。此前绑定在旧成员上的事件、系统与定时器将停止执行。

加载在替换前会先完成预检与新成员观察存储准备：一旦预检失败（如超出硬上限或 Map 存在重复键），原有数据集保持原封不动。可通过 `CollectionLoadError::error()` 查看失败原因，并通过 `into_input()` 无克隆地取回已消费的前缀、触发失败的条目以及剩余迭代器：

```rust,ignore
if let Err(failure) = todo.items().load(records) {
    let reason = failure.error();
    let recovered = failure.into_input();
    handle_rejected_import(reason, recovered);
}
```

## 归并列表数据 (`reconcile`) {#merging-list-data}

`items.reconcile(records, key_of, merge)` 使用业务键（`K: Ord`）将新传入的数据与现有成员进行比对：匹配上的成员**保留原有的 `ModelItemId`、观察者与副作用处理器**，缺失的成员被退役，新增的键分配新身份，最终列表顺序更新为输入顺序。若任一侧存在重复键，整批归并在运行 `merge` 闭包前即被拒绝并通过 `CollectionLoadError` 返还输入。

```rust,ignore
todo.items().reconcile(
    records,
    |item| item.id,
    |current, incoming| {
        current.title = incoming.title;
    },
)?;
```

`merge` 闭包接收 `&mut Item` 与新迁入的 `Item` 并返回模型的变更值（默认模型返回 `()`）。未在闭包中覆盖的字段保留原值。被观察字段会在归并结束后自动比对最终值，若顺序未变且仅合并了字段，则不会发布多余的结构变更通知。

## 归并键值映射 (`map.reconcile`) {#merging-keyed-data}

`map.reconcile(entries, merge)` 直接使用 `ModelMap` 自身的键进行比对，无需额外传入键提取函数。输入为 `(key, data)` 对的迭代器：匹配键保留原有身份、存储键、观察者与副作用处理器，新键创建新成员，消失的键退役旧成员，遍历顺序始终保持按键有序：

```rust,ignore
workspace.users().reconcile(records, |current, incoming| {
    current.name = incoming.name;
})?;
```

## 成员资格集合 (`ModelSet`) {#membership-sets}

对于多选 ID、已启用开关项或标签集合，可声明 `selected: model!(owned ModelSet<UserId>)`。`ModelSet::from_iter(values)` 会移动输入并去重排序；`bounded(n)` 准备带硬上限的空集合。集合成员是不可变值而非模型，没有独立的成员句柄（若存储对象需要可变字段或方法，请改用 `ModelMap`）。

- `contains(&key)`、`len()` 与 `is_empty()` 均可建立响应式观察；
- `try_insert(value)` 插入新值返回 `Ok(true)`，重复值返回 `Ok(false)`，容量不足返回 `Err((error, value))`；
- `remove(&key)` 返回是否实际移除了元素（移除不存在的值或插入重复值不会发布变更通知）；
- `for_each` 按排序顺序借用每个值；`to_vec()` 则要求 `K: Clone` 并创建一份显式的自有快照；
- `load(values)` 替换并去重安装新集合，而 `reconcile(values)` 保留相等的已有值并仅发布成员资格变化。硬上限按去重后的不同成员数计算。
