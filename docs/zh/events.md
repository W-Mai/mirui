# 手势识别与事件回调 {#events-and-callbacks}

本文介绍如何声明强类型事件载荷、绑定回调以及配置有界的通知与命令存储。关于控件自身的输入钩子与文本编辑通知，请参阅[控件初始化、输入与通知](widget-events.md)；关于文档编辑与输入法会话，请参阅[文本编辑与输入法](text-editing.md)；关于选区快照与系统剪贴板读写，请参阅[跨平台剪贴板](clipboard.md)。

<div class="arch-diagram" role="img" aria-label="从原始指针/按键输入到手势识别、冒泡/通知路由、#[event] 回调与延迟命令提交的四阶段流水线">
  <svg viewBox="0 0 880 182" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="ev-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="184" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · RAW INPUT</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">命中测试与手势识别</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Tap(n) · LongPress</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Drag · Pinch · Rotate</text>
    <text x="32" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">派发前自动刷新过期几何</text>
    <path d="M200 91H240" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#ev-arrow)" />
    <rect x="248" y="22" width="212" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · ROUTE DISPATCH</text>
    <text x="264" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">强类型事件路由</text>
    <text x="264" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">EntityBubble (目标 → 根)</text>
    <text x="264" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">EntityNotification (控件通知)</text>
    <text x="264" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">InputFallback → App 监听器</text>
    <path d="M460 91H500" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#ev-arrow)" />
    <rect x="508" y="22" width="184" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="524" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · #[event] HANDLER</text>
    <text x="524" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">受控作用域执行</text>
    <text x="524" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">event!(E) · entity!()</text>
    <text x="524" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">model!(T) · query!(...)</text>
    <text x="524" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">BubbleControl::Allow / Stop</text>
    <path d="M692 91H732" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#ev-arrow)" />
    <rect x="740" y="22" width="124" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="756" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · COMMIT</text>
    <text x="756" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">延迟命令提交</text>
    <text x="756" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">commands!()</text>
    <text x="756" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Model 副作用</text>
    <text x="756" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">在下一条输入前落定</text>
  </svg>
</div>

## 事件类型契约 (`Event` 与 `EventKey`) {#event-type-contracts}

选区请求使用 `ui::selection::SelectAll` 并返回 `Result<(), RequestError>`。调用 `commands.selection().select_all(entity)` 会在目标节点上调用唯一的提供者（不冒泡、不走 `App` 回退、也不占用通知队列存储）。`TextEditing` 默认注册了该提供者；自定义组件也可以在 `View` 的 `input` 元组中注册 `#[event]` 回调，或在具体节点上用 `on SelectAll` 覆盖默认实现。请求类绑定不会给节点增加指针高亮反馈或命中测试目标。

`input::event::contract::Event` 声明事件载荷的**路由（Route）**、**事件源约束（Source）**与**配置选项（Options）**。`EventKey<E>` 仅存储配置选项并保留载荷类型标记（当 `Options = ()` 时为零大小类型）。

自定义控件通知使用 `EntityNotification` 路由与 `WidgetSource<W>` 事件源约束：

```rust
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};

#[mirui::component]
struct Pad;

struct Hit {
    strength: u8,
}

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}
```

`WidgetSource<Pad>` 编译器级约束确保该通知只能安装在挂载了 `Pad` 的控件上，而不能误挂到其他控件或 `App` 监听器上。四类核心路由分工明确：
- **`EntityBubble`**：从命中目标向祖先节点冒泡的手势与按键事件（支持返回 `()`、`bool` 或 `BubbleControl` 控制是否继续冒泡）；
- **`EntityNotification`**：控件向自身监听器派发的业务通知（返回值固定为 `()`）；
- **`InputFallback`**：经焦点或滚动处理后未被消费、最终送达 `App` 监听器的回退输入；
- **`AppNotification`**：不可消费的应用生命周期通知（`Started`、`Suspended`、`Resumed`、`MemoryWarning`、`HostInteraction`、`Quit`）。

内置控件通知位于各自的模块下：`slider::ValueChanged`、`slider::DragStarted`、`slider::DragEnded`、`slider::DragCancelled`、`switch::Toggled`、`checkbox::Toggled`、`progress_bar::ValueChanged`、`tabbar::SelectionChanged` 与 `text_editing::Changed`（可通过 `Slider::ValueChanged` 等类型常量直接引用）。

常用手势导出在 `prelude` 中：`Tap`、`LongPress`、`DragStart`、`DragMove`、`DragEnd`、`DragCancel`、`Pinch` 与 `Rotate`。其中默认 `on Tap` 匹配任意点击次数；若传入显式连击次数 `on Tap(2)`，则只匹配对应的连击重数（有效范围为 `1..=255`）。

`ui!` 中的 `on` 绑定统一支持四种写法：**内联代码块**（通过 `event` 变量读取载荷）、**单参数闭包**、**普通 Rust 函数**以及 **`#[event]` 具名回调工厂**（支持注入 `event!(E)`、`entity!()`、`model!(T)`、`res!(...)`、`query!(...)` 与 `commands!()`）：

```rust,ignore
Button(text: "DROP") on Tap { game.drop_ball(); }

View () on Tap(|input| { game.select_at(input.x, input.y); })

View () on DragMove(|input: event!(DragMove), node: entity!(), items: query!(mut Draggable)| {
    if let Some(item) = items.get(node) {
        item.move_by(input.dx, input.dy);
    }
})

View () on DragMove(moved::callback(game, step))

View () on Tap(handle_tap)
```

同一个节点上的不计次 `Tap` 与计次 `Tap(n)` 按声明顺序依次执行，并在第一个消费事件的处理器处停止。返回 `false`（或 `BubbleControl::Allow`）表示允许后续处理器继续运行；返回 `()` 或 `true` 则消费并终止：

```rust,ignore
View ()
    on Tap { game.record_tap(); false }
    on Tap(2) { game.record_double_tap(); }
```

`Key` 事件同时携带与输入法布局无关的物理键位 `physical: Option<PhysicalKey>`（如美式键盘位置上的 `PhysicalKey::KeyW`）以及逻辑键码 `code`。`Wheel` 与 `Rotary` 仅支持安装在 `App` 级回退监听器上。

## 布局尺寸变更事件 (`SizeChanged`) {#layout-size-events}

`SizeChanged` 在节点首次算出有效逻辑尺寸以及后续宽度或高度发生变化时触发。首次投递时 `event.old` 为 `None`，`event.new` 为包含 `width` 与 `height` 的 `LayoutSize`。单纯的位置平移、滚动偏移、输出缩放比或视觉变换不会触发尺寸事件；隐藏或未挂载节点不参与测量。

```rust,ignore
View(width: Dimension::percent(100), height: 360) on SizeChanged {
    model.set_stage_size(event.new.width, event.new.height);
}

View() on SizeChanged(|change: event!(SizeChanged)| {
    model.set_stage_size(change.new.width, change.new.height);
})

#[event]
fn resized(model: model!(PanelModel), change: event!(SizeChanged)) {
    model.set_stage_size(change.new.width, change.new.height);
}

View() on SizeChanged(resized::callback(model.clone()))
```

`SizeChanged` 由布局阶段直接产生，不冒泡、不增加命中测试目标，且其引起的响应式状态更新会直接参与同一帧的收敛结算。若只需跨节点引用布局几何，请优先使用零分配的 `@` 几何响应式属性；若需要将容器宽高同步进业务 `Model`（例如根据画布尺寸重算贝塞尔路径范围），则使用 `SizeChanged`。

## 应用级回调与事件容量预算 (`EventCapacity`) {#app-callbacks-and-event-capacity}

`App::on_event(E::EVENT, callback)` 用于安装应用生命周期通知或全局按键/滚轮回退监听器：

```rust
use mirui::prelude::*;

#[model]
struct Shortcuts {
    enabled: bool,
}

#[model]
impl Shortcuts {
    fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

#[event]
fn handle_key(input: event!(Key), shortcuts: model!(Shortcuts)) -> bool {
    if input.pressed && input.code == mirui::input::event::input::KEY_RETURN {
        shortcuts.toggle();
        true
    } else {
        false
    }
}

let mut app = App::headless(320, 240);
let shortcuts = app.add_model(Shortcuts { enabled: false }).finish();
app.on_event(Key::EVENT, handle_key::callback(shortcuts));
```

当存在聚焦的编辑器时，节点按键回调与默认文本编辑优先于 `App` 全局按键回退执行；滚轮与旋钮事件同样优先尝试滚动路由，只有在未被滚动容器消费时才会落入 `App` 监听器。

`App` 默认自带一套按需分配的有界事件容量预设（双缓冲各 8 条业务通知记录、每次回调最多 5 条延迟命令记录、单次外层刷新最多投递 16 条通知）。如果应用需要更大的自定义通知载荷或更长的命令批次，可在安装第一个回调前通过 `App::with_event_capacity` 显式配置：

```rust,ignore
app.with_event_capacity(
    EventCapacity::new()
        .notifications::<PadHit>(MAX_PENDING_HITS)
        .commands(CommandCapacity::new().emit::<PadHit>(MAX_HITS_PER_CALLBACK))
        .flush_limit(MAX_DELIVERIES),
);
```

第一个回调成功安装后，容量配置即被冻结；派发期间绝不会在后台隐式扩容已准备好的缓冲区。

## 通知载荷双缓冲存储 {#notification-payload-storage}

内部通知服务使用两块显式预留的缓冲区（双缓冲乒乓切换）：当回调正在稳定读取当前批次的只读载荷窗口时，回调内部新发出的通知会追加到另一块写入缓冲区中，并在当前批次剩余记录全部执行完毕后按 FIFO 顺序继续排空，从而在不发生递归重入的前提下保证严格的先进先出顺序。在预留容量上限内的写入与排空全程零堆分配。

## 已识别的手势批次与状态机 {#recognized-gesture-batches}

手势识别器在同一输入批次中统一维护指针状态：
- **点击与长按（`Tap` / `LongPress`）**：手指按下后在移动阈值内抬起触发 `Tap`（快速连点递增连击计数 `count`）；按住超过长按阈值且未发生拖拽时触发 `LongPress`；
- **拖拽（`DragStart` / `DragMove` / `DragEnd` / `DragCancel`）**：位移越过阈值后锁定手势目标并持续派发 `DragMove`，直至松手（`DragEnd`）或被窗口失焦/挂起打断（`DragCancel`）；
- **双指缩放与旋转（`Pinch` / `Rotate`）**：根据多点触控距离比与角度增量派发。

## 回调参数校验与延迟命令执行 {#prepared-callback-execution}

`#[event]`、`#[system]` 与 `#[timer]` 共用同一套编译期与安装期访问计划校验器：若参数中存在组件/资源可变借用冲突，或混入了属于其他 `App` 的模型句柄，安装阶段即会拦截并输出结构化错误日志。

在回调执行期间，`commands!()` 先将操作记录在当前回调专属的定长命令缓冲中；待回调函数体返回、所有参数借用全部释放后，执行器再按源码顺序依次提交命令。

## 监听器选择与路由快照 {#listener-selection-and-notification-delivery}

派发手势或键盘事件前，监听器池会先冻结当前的 `Parent` 祖先链与有效监听器快照，并在每一步执行前核对实体代数（generation）：
- 若回调中途移动了节点父子关系、增删了监听器或移除了过滤组件，当前正在冒泡的这一轮路由快照不会被篡改；
- 隐藏或禁用（disabled）的节点及其祖先在路由选择阶段即被跳过，不会触发手势或焦点按键，但业务通知（`EntityNotification`）仍然可以送达存活的隐藏/禁用控件。

## 延迟发送自定义控件通知 (`commands.emit`) {#deferred-widget-notification-submission}

在自定义控件的输入回调中，使用 `commands.emit(entity, payload)` 向该节点发送业务通知：
- `commands.emit` 仅在命令提交时刻该实体确实安装了对应类型的监听器时才将载荷移入通知队列；若无人监听则直接丢弃，不消耗通知缓冲区配额；
- `commands.emit_always` 则无论当前是否有监听器都会校验实体、事件源与容量配额。两者均需在自定义容量配置中通过 `CommandCapacity::emit::<E>(count)` 预留额度。

## 具名事件回调声明 (`#[event]`) {#named-event-callback-declarations}

`#[event]` 生成一个带有 `callback(...)` 工厂函数的同名模块。工厂函数按参数声明顺序接收已注册的 `Model` 实例以及普通的 `Copy + 'static` 值；而资源（`res!`）、查询（`query!`）、事件载荷（`event!`）、当前实体（`entity!`）与延迟命令（`commands!`）则在事件触发调用时由框架注入，无需传给工厂函数：

```rust,ignore
#[event]
fn moved(
    game: model!(Game),
    step: u16,
    input: event!(DragMove),
    node: entity!(),
    positions: query!(mut Position),
) -> bool {
    let Some(position) = positions.get(node) else {
        return false;
    };
    game.move_to(position, input.x, input.y, step);
    true
}
```

调用 `moved::callback(game, step)` 即可构造回调描述符，无需暴露 `World` 或生成的句柄类型。通知类回调返回 `()`；可消费的冒泡或回退路由还支持返回 `bool` 或 `BubbleControl`。

