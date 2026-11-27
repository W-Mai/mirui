# 组合、角色与自定义视图 {#composition-roles-and-custom-views}

本文讲解 `#[compose]` 组合函数、资源借用（`res!`）、无分配属性转发（`prop!`）、ECS 系统（`#[system]`）以及自定义视图描述符（`#[view]` / `ui!(compose Name)`）如何协同工作。

关于响应式模型、信号与样式表，请参阅[响应式状态与样式](state-management.md)。关于事件载荷路由与手势识别，请参阅[手势识别与事件回调](events.md)。

<MiruiSnippet recipe="layout" label="Composition & Flexbox" file="recipes.rs" height="230px">

<template #exercise>

点击 `Toggle width` 或拖动右下角手柄缩放视口——左侧主卡片在 `96 px` 与 `156 px` 之间切换，而右侧 `grow: 1.0` 的兄弟容器自动占满剩余行宽。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="从 App::compose 到 ECS 系统与自定义 View 的四阶段角色流转">
  <svg viewBox="0 0 940 186" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="comp-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="30" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">01 · #[compose]</text>
    <text x="30" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">界面树与属性输入</text>
    <text x="30" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">model!(T) · prop!(T)</text>
    <text x="30" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">res!(T) · id("name")</text>
    <text x="30" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">栈上持有，在 App::compose 执行</text>
    <path d="M224 93H244" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="250" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">02 · #[event] / #[timer]</text>
    <text x="264" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">输入与定时回调</text>
    <text x="264" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">event!(Tap) · entity!()</text>
    <text x="264" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">query!(...) · commands!()</text>
    <text x="264" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">修改 Model 并排入延迟命令</text>
    <path d="M458 93H478" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="484" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="498" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">03 · #[system] / #[animate]</text>
    <text x="498" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">帧调度系统</text>
    <text x="498" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">query!(mut C) · motion!()</text>
    <text x="498" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">flush_signal_dirty</text>
    <text x="498" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">推进弹簧补间与布局</text>
    <path d="M692 93H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="718" y="24" width="206" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="732" y="52" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">04 · #[view]</text>
    <text x="732" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">绘制与脏区失效</text>
    <text x="732" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">component!()</text>
    <text x="732" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">paint!() · watch(...)</text>
    <text x="732" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">提交 DrawCommand</text>
  </svg>
</div>

---

## 选择状态与渲染角色 {#choosing-state-and-rendering-roles}

| 角色 | 适用场景 | 更新链路 |
| --- | --- | --- |
| `#[model]` | 已注册的业务状态（支持同一类型注册多个实例） | 生成的句柄方法比对 `#[observe]` 前后值、发布具名修订号并派发强类型副作用 |
| `Signal<T>` | 独立的小型响应式值，或由其他所有者发布的值 | `set` / `update` 直接通知响应式读取者 |
| `#[component]` | 挂载在实体上的数据 | World 插入与替换驱动组件生命周期；`model!(T)` 字段持有已注册模型的身份 |
| `#[compose]` | 构建一棵控件子树 | `model!(T)` 参数共享实例；`prop!(T)` 输入把静态值或读取器零拷贝转发给最终消费的绑定 |
| `com!(entity, T)` | 在组合期间读取实体组件 | 返回短暂的 `Option<&T>` 借用（不建立订阅）；事件回调中请改用 `query!` 参数 |
| `View` / `#[view]` | 注册一种控件类型（非空函数体提供绘制逻辑，空函数体仅注册行为） | 独立的钩子提供绘制、挂载初始化、手势、观察与系统；强类型角色声明模型投影与资源 |
| `#[system]` | 按帧调度的批量更新 | `model!(T)` 捕获已注册模型实例；`res!`、`query!` 与 `commands!` 声明其余运行时访问 |
| `on Event { ... }` | 处理手势与控件业务事件 | `event` 借用强类型事件载荷；`commands!()` 推迟结构变更；回调参数提供资源与查询访问 |
| `#[timer]` | 按时间计划运行回调 | `model!`、`entity!` 与 `commands!` 声明回调所需的运行时访问 |
| `#[animate]` | 将标量过渡运动（`Motion`）应用到组件字段 | `motion!`、`component!`、`model!`、`entity!` 与 `commands!` 声明逐帧访问 |
| `AudioHandle` | 启用 `audio` 特性时的共享音频输出控制 | 宿主 UI 与演示控件读取同一输出状态并提交至同一个有界音频总线 |
| `World` | 底层实体、组件与资源直接访问 | 供底层集成代码使用；日常模型方法与 UI 回调无需触碰 `World` |

`#[model]` 并非用来替代 ECS 组件：模型拥有应用级业务身份，而组件将行为与模型绑定挂载到具体的控件实体上。`#[view]` 也并非用来替代 `#[compose]`：`#[view]` 声明一种控件的行为与可选绘制回调，而 `#[compose]` 负责拼装外围的控件树。`Signal<T>` 对于独立的轻量状态依然顺手，不必为了一个布尔值强行包装一层模型。

`#[system]`、`#[event]`、`#[timer]`、`#[view]` 与 `#[animate]` 共享统一的回调属性路由规则：条件编译属性（`#[cfg]`）会应用到宏生成的每一个条目；`#[inline]` 等函数属性应用到回调本体；文档注释则应用到生成的注册模块或动画组件上。`cfg_attr` 会按此规则自动拆分，非法或嵌套错误的属性会在编译期直接报错。

系统、事件、定时器与视图工厂在生成注册模块时会完整保留源码中书写的 `self::` 与 `super::` 相对路径（包括角色类型、访问计划、视图泛型约束与钩子以及定时器计划表达式），而函数体保留在原声明作用域内。泛型视图会显式转发类型参数与常量泛型参数。

---

## 组合函数与 `prop!(T)` 输入 {#compositions-and-prop-inputs}

调用 `#[compose]` 函数只会在栈上准备一个待执行的组合值，此时函数体尚未运行；所有传入的参数都由该栈上值持有或借用，直到交给 `App::compose` 执行或直接丢弃。模型校验、资源访问、属性绑定与实体创建均在 `App::compose` 内部执行；因此，单纯构造一个组合值并不要求当前线程已经持有活动的应用上下文。组合函数可以返回任意具体的 Rust 类型（包括泛型与借用调用方传入参数的引用，但不能逃逸出构建作用域本身的临时资源借用）。

`App::compose` 执行完毕后返回组合函数的返回值（如根实体 `Entity` 或自定义结构体）。在该次调用中创建的 `Computed` 与 `Effect` 均归属于该 `App` 实例。构建作用域在返回前即告结束，响应式所有权随之恢复并备好脏队列存储。

建议将视觉背景与内边距设置在组合函数自己创建的根节点上，避免篡改调用方父容器的样式。

### 零分配属性转发 (`Prop<T>` 与 `prop!(T)`) {#zero-allocation-prop-forwarding-prop-t-prop-t}

`Prop<T>` 可以携带一个静态值或一个共享的只读读取器（getter）。`Prop::from(value)` 直接存储静态值；`Prop::new(move || source.get())` 则保留读取闭包而不立即求值。调用 `get()` 时会在当前调用方的响应式作用域内求值，因此最底层控件的绑定会直接订阅到最原始的信号或模型字段。克隆动态 `Prop` 只会共享同一个读取闭包，不会额外分配内存或注册中间层的观察消费者。每次读取都会运行一次 getter；若计算开销较大且需要缓存，请配合 `Computed` 使用。

在 `#[compose]` 参数中使用 `prop!(T)` 声明属性输入。调用方既可以传入普通静态值，也可以传入 `${expression}` 响应式读取器。中间层组合函数可以直接把 `text` 原样向下转发给子组合函数，直到最内层的 `Text(text: ${text})` 在其属性绑定中真正读取它——中间层不注册任何多余的 `Effect`，且当数据源变化时只更新最内层文本，绝不会重建整棵子树。静态值与 `${expression}` 的结果都会自动通过 `Into` 转换为目标类型（因此 `prop!(String)` 同时接受 `&str` 与 `String`）：

```rust
use mirui::prelude::*;
use mirui::ui::widgets::Text;

#[compose]
fn caption(text: prop!(String)) -> Entity {
    ui! { Text(text: ${text}) }
}

#[compose]
fn header(text: prop!(String)) -> Entity {
    ui! { caption(text) }
}

#[compose]
fn panel(title: model!(Signal<String>)) {
    ui! {
        Column {
            header("Static subtitle")
            header(${title.get()})
        }
    };
}
```

---

## 应用级资源 (`res!`) {#application-resources}

`App::add_resource(value)` 将全局资源直接存入应用的 `World` 存储中。资源值直接移动（move）进入存储，不要求实现 `Clone` 或 `Copy`；重复插入同一类型会替换并释放旧值。各类回调通过 `res!(T)` 或 `res!(mut T)` 借用当前资源；若需在回调期间插入或移除资源，请使用 `commands.insert_resource(value)` 与 `commands.remove_resource::<T>()` 在参数借用结束后延迟执行。

在 `#[compose]` 函数的 Rust 代码块中，可直接使用 `res!(...)` 表达式访问当前应用的资源存储。每次 `.with(...)` 闭包仅在调用期间借用资源，并返回脱离资源引用的独立值：

| 操作 | 表达式 | 结果 |
| --- | --- | --- |
| 读取必需资源 | `res!(T).with(read)` | `read` 接收 `ResourceRef<'_, T>` |
| 读取可选资源 | `res!(optional T).with(read)` | `read` 接收 `Option<ResourceRef<'_, T>>` |
| 修改必需资源 | `res!(mut T).with(write)` | `write` 接收 `ResourceMut<'_, T>` |
| 插入或替换资源 | `res!().insert(value)` | 立即替换并释放旧值 |
| 移除资源 | `res!().remove::<T>()` | 返回 `Option<T>` |

```rust,ignore
struct PanelConfig {
    gap: i32,
}

#[compose]
fn configured_panel() {
    let gap = res!(PanelConfig).with(|config| config.gap);
    ui! {
        Column (gap: gap) {}
    };
}

app.add_resource(PanelConfig { gap: 8 });
app.compose(root, configured_panel());
```

只有 `res!(optional T)` 允许目标资源不存在。对于应用初始化阶段必定安装的依赖，请使用必需资源角色；当“资源尚未安装”本身属于正常分支逻辑（例如检查时钟插件是否启用）时，再使用 `optional`。解引用资源绝不会隐式构造默认值。

请先在局部变量中通过 `res!(...).with(...)` 算出所需值，再传给 `ui!`、`format!` 等其他宏——`#[compose]` 不会深入改写其他外部宏内部的标记。

---

## 具名控件身份 (`id`) {#named-widget-identities}

`App` 在创建时自带具名控件 ID 注册表。在 `ui!` 中声明 `id: "name"` 即可登记实体身份，且已登记的名称对后续的 `compose` 调用持续可见。

在同一个静态兄弟节点序列中，`id("source")` 或 `id(source)` 支持**前向引用**排在后面的具名控件（包括在排在前面的兄弟节点的子树或子组合参数中引用它）。宏会在执行该序列前先行预留目标控件的 `Entity` ID，随后按声明顺序依次初始化组件、挂载视图并追加子节点。注意：前向引用拿到的是 `Entity` 标识，此时目标控件上的组件尚未完成初始化。在条件分支（`if` / `match`）、循环体（`walk`）、子组合函数或插槽内部声明的 `id` 不会跨越各自的作用域向外预留。控件具名 ID 在整个应用内必须保持唯一。

```rust,ignore
ui! {
    View () {
        View () [DropShadow::new(id("source"))]
        View (id: "source", width: 80, height: 40)
    }
}
```

---

## 命令式构建闭包 (`App::compose` closures) {#programmatic-builders}

如果不使用 `ui!` 宏，也可以直接向 `App::compose` 传入闭包来命令式构建界面树。其作用域支持直接挂载组件、强类型控件构建器（builder）以及两者的元组。`children` 方法返回容器实体以及闭包的返回值：

```rust,ignore
let (panel, slider) = app.compose(root, |tree| {
    tree.children(panel_style(), |column| {
        column.spawn(Text::build("Level").style(label_style()));
        let source = controls.clone();
        let slider = column.spawn(
            Slider::build(0, 100)
                .bind_property::<prop::SliderValue>(move || source.level())
                .fill_color(ColorToken::Primary)
                .style(slider_style())
                .on_event(Slider::ValueChanged, level_changed::callback(controls.clone())),
        );
        column.spawn((
            Button::build().on_event(Tap::EVENT, reset::callback(controls.clone())),
            Text::label("Reset"),
        ));
        slider
    })
});
```

强类型构建器在链式调用 `bind_property`、`on_event` 与 `style` 时全程保留具体控件类型。`tree.on_event(entity, key, callback)` 可在已创建的实体节点上挂载事件回调并校验其事件源类型。

---

## 系统、查询与定时器 (`#[system]`、`#[timer]`、`#[animate]`) {#systems-queries-and-timers}

`#[system]` 函数通过参数角色显式声明所需的模型、资源、组件查询与延迟命令，无需接收裸 `World`：

```rust,ignore
#[system(order = Animation)]
fn advance(
    counter: model!(Counter),
    delta: res!(mirui::ecs::DeltaTimeMs),
) {
    for _ in 0..delta.0 {
        counter.increment();
    }
}

app.add_system(advance::system(counter.clone()));
```

查询参数（`query!`）除了支持遍历之外，还支持通过 `rows.get(entity)` 按实体 ID 定点读取：

```rust,ignore
#[system]
fn update_selected(
    selected: res!(SelectedEntity),
    rows: query!(mut Position, optional Velocity),
) {
    if let Some((position, velocity)) = rows.get(selected.0) {
        if let Some(velocity) = velocity {
            position.x += velocity.x;
        }
    }
}
```

### 定时器 (`#[timer]`) {#timers-timer}

`#[timer]` 声明触发计划（`after = ms`、`every = ms`、`repeat = (count, ms)` 或 `until = (deadline, ms)`）以及回调参数角色：

```rust,ignore
#[timer(every = 1_000)]
fn tick(
    counter: model!(Counter),
    timer: entity!(),
    commands: commands!(),
) {
    counter.increment();
    commands.insert(timer, Selected);
}

app.add_timer(tick::timer(counter));
```

### 标量动效应用器 (`#[animate]`) {#scalar-motion-applicators-animate}

`#[animate]` 定义如何将一段标量 `Motion`（弹簧或补间）写入目标组件字段，并自动生成具名动画组件与对应的系统构造器：

```rust,ignore
use mirui::anim::{Motion, Tween};

#[component]
struct Opacity(Fixed);

#[animate(component = AnimateOpacity)]
fn apply_opacity(value: motion!(), opacity: component!(mut Opacity)) {
    opacity.0 = value;
}

app.add_system(AnimateOpacity::system());
```

---

## 自定义视图 (`#[view]`) {#custom-views}

`#[view]` 根据指定的组件类型生成自定义控件视图，并根据参数角色自动建立模型观察订阅：

```rust,ignore
#[view(component = CounterTile, priority = 60, invalidation = Visual)]
fn paint_counter(
    counter: model!(component.counter, watch(count)),
    entity: entity!(),
    paint: paint!(),
) {
    let _ = (counter.count, entity, paint.rect());
}

app.with_widget(paint_counter::view());
```

- `model!(component.field, watch(selector))` 在绘制期间借用组件中持有的模型，并自动订阅指定的 `#[observe]` 字段、派生方法或具名修订号。
- `invalidation = Layout` 将所观察的变更提升为布局级失效（默认 `Visual` 仅触发视觉重绘）。
- `attach = initialize::callback()`、`input = (tap::callback(), key::callback())` 与 `systems = [...]` 用于声明控件挂载钩子、输入处理能力与伴随系统。
- 自定义控件还可以在组件字段上标注 `#[property(P)]`（其中 `P: Property`），使其在 `ui!` 中直接支持 `$` 响应式属性绑定。
- 空函数体的 `#[view]`（如 `TabPages` 或 `LazyList`）只注册挂载钩子、观察与系统，不注册绘制回调。

---

## 可复用插槽组件 (`ui!(compose Name)`) {#reusable-niches}

`ui!(compose Name { ... })` 声明一个带子树结构与具名插槽（`@@slot_name` 声明槽位 / `@slot_name { ... }` 填充槽位）但无需自定义绘制钩子的插槽组件（Niche）。通过 `app.with_widget(ui!(compose Name))` 注册后即可在 `ui!` 中像原生控件一样使用：

<MiruiSnippet recipe="compose-niche" label="ui!(compose Name)" file="recipes.rs" height="190px">

<template #exercise>

点击 `@body` 插槽内的 `Toggle` 按钮——`StatusCard` 更新顶部的 `title: prop!(String)`，同时在 `@@body` 位置承载调用方传入的插槽子树。

</template>

</MiruiSnippet>
