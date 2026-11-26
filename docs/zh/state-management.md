# 响应式状态与样式 {#reactive-state-and-styles}

`mirui` 通过 `#[model]` 实例、`Signal<T>`、`Computed<T>` 与 `StyleSheetPlugin` 声明式地驱动控件属性与界面树结构。只要修改模型字段或信号，下一帧便只更新读取了该数据的控件——无需手写树比对或手动接线观察者。

关于 `#[compose]` 函数、`res!(T)` 资源借用、`prop!(T)` 转发、ECS 系统与自定义 `#[view]` 绘制描述符，请参阅[组合、角色与自定义视图](composition.md)。关于自动落盘与启动恢复，请参阅[状态快照与自动落盘](persistence.md)。

<MiruiSnippet recipe="reactive" label="Model" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-reactive{rust}

<template #exercise>

点击 `+ 10` 或 `- 10` 修改 `Gauge::level`——数字读数、百分比填充条宽度以及满载告警色会在同一帧内同步更新。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="响应式事务与单帧合并刷新流水线">
  <svg viewBox="0 0 880 172" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="state-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="192" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · MUTATION</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Model / Signal 写入</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">gauge.step(10)</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">signal.set(next)</text>
    <text x="32" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">比对 #[observe] 前后快照</text>
    <path d="M208 86H246" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="254" y="22" width="192" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="270" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · DIRTY QUEUE</text>
    <text x="270" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">订阅者去重入队</text>
    <text x="270" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">${ gauge.level() }</text>
    <text x="270" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Computed&lt;T&gt; · Effect</text>
    <text x="270" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">同一帧多次修改自动合并</text>
    <path d="M446 86H484" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="492" y="22" width="182" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="508" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · FLUSH</text>
    <text x="508" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">flush_signal_dirty</text>
    <text x="508" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Property::apply</text>
    <text x="508" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">StyleSheet 样式类切换</text>
    <text x="508" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">仅更新实际变化的属性</text>
    <path d="M674 86H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="720" y="22" width="144" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="736" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · DAMAGE</text>
    <text x="736" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">布局与脏区重绘</text>
    <text x="736" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">PrevRect ∪ CurrRect</text>
    <text x="736" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">render_dirty()</text>
    <text x="736" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">只刷新受影响的像素</text>
  </svg>
</div>

## 目录 {#contents}

1. [已注册的模型实例 (`#[model]`)](#registered-model-instances)
   - [内联子模型 (`model!(owned Child)`)](#owned-child-models)
   - [模型角色参数 (`model!(T)`)](#model-roles)
2. [响应式原语 (`Signal`、`Computed`、`Effect`)](#primitives)
3. [可复用样式 (`style!` 与 `StyleSheetPlugin`)](#reusable-styles)
   - [样式表类规则](#stylesheet-classes)
4. [响应式属性 (`$`)](#reactive-attributes)
5. [几何响应式属性 (`@`)](#layout-responsive-attributes)
6. [响应式控制流 (`if $`、`match $`)](#reactive-control-flow)
7. [列表迭代：`walk`、按位置与按键值](#lists)
8. [单帧合并刷新机制](#the-flush-model)
9. [边界条件与契约](#limits)

---

## 已注册的模型实例 {#registered-model-instances}

在结构体与对应的固有 `impl` 块上标注 `#[model]`，即可生成其实例句柄与转发方法。每次调用 `app.add_model(value).finish()` 都会注册一个独立的模型实例；克隆句柄只会共享同一个实例，不会复制底层业务数据。

```rust
use mirui::prelude::*;

#[model]
struct Counter {
    #[observe]
    count: u32,
}

#[model]
impl Counter {
    #[observe]
    fn is_even(&self) -> bool { self.count % 2 == 0 }

    fn increment(&mut self) { self.count += 1; }
    fn decrement(&mut self) { self.count = self.count.saturating_sub(1); }
}

let mut app = App::headless(32, 32);
let counter = app.add_model(Counter { count: 0 }).finish();
counter.increment();
```

- **字段读取器 (`#[observe]`)**：`#[observe]` 会为实现了 `Clone + Eq` 的字段（包含 `String` 与 `Vec<T>`）自动生成同名读取方法。在 `${...}` 内部调用 `counter.count()` 即可让当前控件属性订阅该字段。若字段最终值未变（或在同一事务内改完又改回原值），不会通知订阅者。字段读取器返回值的克隆副本，不直接借用模型内部内存。
- **派生读取器 (`#[observe] fn`)**：在接收 `&self`、无额外参数且返回值满足 `Clone + Eq` 的方法上标注 `#[observe]`，只有当计算结果发生变化时才会向外发布更新。
- **可失败事务 (`Result<Change, E>`)**：可变方法可返回模型的变更类型（默认为 `()`）或 `Result<Change, E>`。一旦返回 `Err`，框架会跳过本次观察发布与修订号递增，并丢弃待发送的副作用。请务必在修改模型字段之前完成校验——返回 `Err` 不会自动回滚已经写入的字段。
- **具名变更掩码 (`watch`)**：对于不适合按值克隆比对的大对象变更，可声明 `#[model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]`。当可变方法返回的 `ChangeSet` 包含 `VISUAL` 位时，`model.visual_revision()` 递增并唤醒所有观察该通道的读取者。
- **业务副作用提取器 (`#[effects]`)**：在 `#[model] impl` 块中声明返回 `Option<E>` 或 `[Option<E>; N]` 的提取方法：

```rust,ignore
#[effects]
fn take_note(&mut self) -> Option<Note> {
    self.pending_note.take()
}

player.on_effect(move |note: &Note| play_note(note.pitch))?;
```

`model.on_effect(handler)` 为每个模型实例的每种事件类型安装一个消费者。副作用在状态发布完毕且模型数据借用释放之后派发；消费者可以读取已提交的最新状态，但不能在回调内开启新的模型写入事务。

### 内联子模型 {#owned-child-models}

在 `#[model]` 结构体内部声明 `child: model!(owned Child)`，即可将子模型内联存储在父模型中。构造时通过 `.into()` 转换原始结构体，且只需在 `App` 上注册最外层的父模型。集合类型同样使用 `model!(owned ModelList<Item>)`、`model!(owned ModelMap<Key, Item>)` 或 `model!(owned ModelSet<Key>)`（详见[有界列表与键值复用](model-collections.md)）。

```rust
use mirui::prelude::*;

#[model]
struct Scoreboard {
    left: model!(owned Counter),
    right: model!(owned Counter),
}

#[model]
impl Scoreboard {
    fn new() -> Self {
        Self {
            left: Counter { count: 0 }.into(),
            right: Counter { count: 0 }.into(),
        }
    }

    #[observe]
    fn total(&self) -> u32 {
        self.left.count() + self.right.count()
    }

    fn increment_both(&mut self) {
        self.left.increment();
        self.right.increment();
    }
}
```

无论通过父句柄还是子句柄调用可变方法，最外层调用结束时都会一次性提交整棵已注册的模型树。所有被观察字段与派生读取器只在最终值确实改变时发布一次通知。

### 模型角色参数 (`model!(T)`) {#model-roles}

`model!(T)` 角色宏让函数签名与组件字段无需暴露底层的生成句柄名。在 `#[component]` 中声明 `model!(T)` 字段可持有一个已注册实例，在 `#[compose]` 函数参数中声明 `model!(T)` 则可接收该实例：

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text};

#[compose]
fn counter_value(counter: model!(Counter), label: &'static str) -> Entity {
    ui! {
        Text(text: ${ format!("{label}: {}", counter.count()) })
    }
}

#[compose]
fn counter_controls(counter: model!(Counter)) {
    ui! {
        Row {
            Button(text: "−") on Tap { counter.decrement(); }
            Button(text: "+") on Tap { counter.increment(); }
            counter_value(counter, "count")
        }
    };
}
```

`ui!` 生成的事件回调与直接调用的子组合函数会自动共享 `counter` 句柄，无需在每个使用处手写 `.clone()`。

---

## 响应式原语 (`Signal`、`Computed`、`Effect`) {#primitives}

`mirui::core::reactive` 提供了三个独立的响应式原语：

```rust
use mirui::core::reactive::{Signal, Computed, Effect};

let count = Signal::new(0i32);          // 持有值并追踪读取者
count.set(1);                           // 替换新值
count.update(|n| *n += 1);              // 原地修改
let n = count.get();                    // 读取并订阅当前上下文
count.with(|n| println!("{n}"));        // 借用读取（不克隆）

let doubled = {
    let count = count.clone();
    Computed::new(move || count.get() * 2)   // 惰性派生，仅在依赖变化且被读取时重算
};

let _effect = Effect::new(move || {
    // 每当内部读取的信号发生变化时重新执行
});
```

- `Signal<T>` 持有独立的响应式值。`get()` / `with()` 将当前读取者注册为订阅方；`set()` / `update()` 将所有订阅方标记为脏。
- `Computed<T>` 惰性派生值：仅在输入源发生变化且自身被读取时重新计算。
- `Effect` 在创建时立即执行一次闭包，并在其读取的任意信号变脏后于帧刷新阶段重新运行。在组合函数内，可使用 `effect_with_widget(entity, closure)` 将副作用的生命周期绑定到具体控件实体。

关于 `#[model]`、`Signal<T>`、`#[component]`、`#[compose]` 与 `#[view]` 的职责对照表，请参阅[选择状态与渲染角色](composition.md#choosing-state-and-rendering-roles)。

---

## 可复用样式 {#reusable-styles}

`style!` 宏在编译期构造强类型的样式预设（`StylePreset`）。通过 `style:` 属性将其应用到控件上，同时可以直接在控件上覆盖个别字段。

```rust,ignore
const STEP: StylePreset = style!(grow: 1.0, height: 44, border_radius: 22);

ui! {
    Row (column_gap: 4) {
        Button (style: STEP, text: "-")
        Button (style: STEP, height: 48, text: "+")
    }
};
```

### 样式表类规则 {#stylesheet-classes}

安装 `StyleSheetPlugin` 时可注册全局共享的样式类规则。控件可以声明单个类名、有序类名列表，或通过 `${...}` 动态切换类名。当多个类设置了同一字段时，排在后面的类生效；控件自身的 `style:` 预设与直接内联书写的字段拥有更高优先级。

<MiruiSnippet recipe="mini-style" label="StyleSheetPlugin" file="recipes.rs" height="200px">

<template #exercise>

点击任意一枚胶囊开关——两枚按钮共用 `"pill"` 基础类，并根据自身状态在 `"pill.on"` 与 `"pill.off"` 之间动态切换。

</template>

</MiruiSnippet>

在 `StyleSheetPlugin` 或 `StyleSheet` 上调用 `.bind_class(name, getter)`，可以让某条样式类规则根据响应式状态动态计算；无论多少个控件引用该类，每次更新只计算一次：

```rust,ignore
#[compose]
fn register_compact_steps() {
    res!(mut StyleSheet).with(|mut styles| {
        styles.set("step", style!(grow: 1.0, height: 36, border_radius: 18));
    });
}
```

---

## 响应式属性 {#reactive-attributes}

在 `ui!` 中，为属性值加上 `$` 前缀即可建立响应式绑定。`attr: $signal` 是 `attr: ${ signal.get() }` 的简写：

```rust,ignore
let label = Signal::new(0i32);

#[compose]
fn counter_label(label: Signal<i32>) {
    ui! {
        Text (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
    };
}
```

当 `label` 变化时，框架只更新该控件的 `text` 属性，不会销毁重建控件。支持响应式绑定的属性涵盖：
- **布局与几何**：`width`、`height`、`left`、`top`、`direction`、`visible`、`scroll_offset`
- **视觉与样式**：`style`、`class`、`bg_color`、`text_color`、`border_color`、`border_width`、`render_key`、`Button.normal_color`
- **文本与路径**：`text`、`font_size`、`Text.paragraph`、`path`
- **交互控件值**：`ProgressBar.value`、`Slider.value`、`Switch.on`、`Checkbox.checked`

通过响应式绑定程序化更新 `Slider.value`、`Switch.on` 或 `Checkbox.checked` 时，控件状态与过渡动画会同步更新，但不会派发用户交互通知事件（`ValueChanged` / `Toggled`）。

---

## 几何响应式属性 {#layout-responsive-attributes}

`$` 响应应用业务状态，而 `@` 则响应布局阶段算出的几何尺寸，并在头部显式列出所依赖的几何源：

<MiruiSnippet recipe="state-at" label="@width & @id(stage)" file="recipes.rs" height="200px">

<template #exercise>

拖动预览框右下角的圆形手柄改变视口宽度，当容器宽度跨过 `320 px` 时，顶部标题通过 `@width` 自动切换字号，底部卡片则通过 `@id(stage).width / 2` 始终保持 `#stage` 一半的宽度。

</template>

</MiruiSnippet>

```rust,ignore
ui! {
    Column (container: true) {
        Text(
            "Status",
            font_size: @width {
                if width < Fixed::from_int(520) { 18_u16 } else { 30_u16 }
            },
            padding: @(width, height) {
                Padding::all(width.min(height) / 24)
            }
        )

        View(id: "stage", width: Dimension::percent(100), height: 180)

        Text(
            "Stage label",
            width: @id(stage).width { stage.width },
            top: @id("stage").height as stage_height { stage_height / 8 }
        )
    }
}
```

不带限定名的 `width` 与 `height` 会向上查找最近一个标记了 `container: true` 的祖先容器；具名依赖（`@id(stage).width`）则通过控件 ID 注册表解析。单个控件最多可声明 4 个不同的几何依赖，全程零运行时堆分配。

---

## 响应式控制流 {#reactive-control-flow}

在控制流头部加上 `$` 即可让分支切换具备响应性：

```rust,ignore
#[compose]
fn status_panel(show: Signal<bool>, other: Signal<bool>, state: Signal<Load>) {
    ui! {
        Column (grow: 1.0) {
            if ${ show.get() } {
                Text ("visible")
            } elif ${ other.get() } {
                Text ("alt")
            } else {
                Text ("hidden")
            }
            match ${ state.get() } {
                Load::Loading => { Text ("loading") }
                Load::Ready(s) => { Text (s) }
            }
        }
    };
}
```

- `if $cond` / `elif` / `else` 会在首次构建时创建所有分支，随后仅通过切换可见性来呈现对应分支，不会丢失各分支内部的控件状态。
- `match $expr` 在所有分支模式都不绑定变量（例如 `Load::Loading`）时会缓存全部分支；只要任意分支绑定了载荷（如 `Load::Ready(s)`），当源值改变时就会重建当前激活的分支，以便分支体拿到最新的载荷值。
- **不带 `$`** 的 `if` / `match` 是纯静态控制流——仅在首次构建界面树时求值一次，后续不再重算。

---

## 列表迭代 {#lists}

`walk` 用于遍历集合生成多行子控件。配合 `$` 使用时，集合内容一变就会自动协调行节点：

- **按位置索引**（`walk ${ items.get() } with item`）：行节点按位置下标对齐。列表变长时在末尾追加新行，变短时销毁末尾多余行。适合只追加或截断尾部的日志列表。
- **按稳定键值**（`walk ${ items.entries() } with row by row.0`）：行节点按稳定身份（`ModelItemId` 或业务键）对齐。当在列表中间插入、删除或重排元素时，存活下来的行会完整保留原有的实体、焦点与动画状态。

关于有界 `ModelList`、`ModelMap` 与 `ModelSet` 的详细接口，请参阅[有界列表与键值复用](model-collections.md)。

---

## 单帧合并刷新机制 {#the-flush-model}

在回调中调用 `signal.set(...)` 或修改 `#[model]` 并不会当场去改写控件树，而是将受影响的订阅者标脏并推入待处理队列。在每一帧的调度流水线中——系统（Systems）与插件更新执行完毕后、布局计算开始前——`flush_signal_dirty` 会一次性排空队列：重跑脏 `Effect` 并应用脏控件属性。同一帧内对同一状态源的多次连续修改会自动合并为单次界面更新。

---

## 边界条件与契约 {#limits}

- **可观察值类型要求**：`#[observe]` 字段与派生方法返回值必须实现 `Clone + Eq`。每个已注册模型实例会为其被观察字段保留一份快照以供事务提交时比对。
- **自定义视图的订阅声明**：在 `#[view]` 参数里仅写 `model!(component.field)` 不会建立自动重绘订阅；必须显式加上 `watch(...)` 指明要监听的字段、派生方法或具名修订号。绘制回调本身是只读的。
- **`Computed` 与 `Effect` 归属权**：在某个 `App` 内创建的 `Computed` 与 `Effect` 归该 `App` 所有，不能跨两个不同的 `App` 实例混读模型。
- **缓存分支约束 (`if $` / `match $`)**：未激活的缓存分支会暂停其显示属性绑定，直到再次被选中；不同分支内部的控件具名 `id` 必须保持全局唯一。若分支内部还嵌套了动态 `if $`、`match $` 或 `walk`，请将它们包裹在一层容器控件内，使动态子节点始终挂在分支根节点之下。
