# 控件初始化、输入与通知 {#widget-initialization-input-and-notifications}

关于回调语法、自定义事件载荷、路由规则及容量配置，请参阅[事件与回调](events.md)。

<MiruiSnippet recipe="vector" label="#[view]" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-vector{rust}

<template #exercise>

点击任意 `VectorGlyph` 瓦片，在 `on Tap` 回调中修改其组件状态并重新描边静态矢量路径。

</template>

</MiruiSnippet>

## 文本文档 {#text-documents}

`mirui::ui::widgets::TextDocument` 独立于编辑器的选区、输入法预编辑（composition）与外观，专门存储 UTF-8 文本内容。它可以通过 `app.add_model(document).finish()` 注册，也可以用 `model!(owned TextDocument)` 嵌套在其他模型内。默认字节容量为 32；调用 `set_capacity` 可显式准备更大的缓冲区。容量变更可独立于内容修订号（revision）被观察。

`TextEditing::default()` 拥有一份本地内联文档；`TextEditing::new(document.clone())` 则直接绑定已注册的文档而不复制内容。每个组件各自维护独立的 `TextSelection`。`read` 在回调期间借用文本内容，`set_selection` 则以原子方式校验起止两个 UTF-8 字节端点。读取选区时若遇到外部修改，会将越界位置钳制到文档末尾或前一个合法 UTF-8 边界，而不会按编辑历史推导位移。注册 `text_editing::view()` 即可通过常规布局失效机制观察共享内容与容量的变更；该视图描述符本身不执行绘制。

在插入节点前，可用 `with_capacity` 与 `with_text` 预置组件状态。若绑定了共享文档，这两个构造辅助方法会直接修改目标文档，而选区变更仍保持在编辑器本地。已注册文档的绑定遵循所在 `App` 的所有权与生命周期规则；当源模型失效时，编辑器不会静默回退为空的本地文档。

`TextEditing` 视图会自动安装焦点与命中测试组件，并独立于外观处理已提交文本片段、删除、光标移动、Shift 扩选以及回车键。它的 `Changed`、`SelectionChanged` 与 `Submitted` 通知复用现有的事件回调体系。外部选区修正在输入处理与渲染前边界处保留。当文档绑定不可用时，输入会被直接拒绝；共享编辑会在触发模型副作用（Model effects）前释放组件借用，并在更新选区前校验数据源身份与修订号。

```rust,ignore
let document = app.add_model(TextDocument::new()).finish();
document.set_capacity(256)?;
document.set_text("Search")?;
document.replace(0..6, "Find")?;
document.read(|text| assert_eq!(text, "Find"));
```

`replace` 接收 UTF-8 字节范围，并在修改内容前拦截非法字符边界、起止倒置的范围以及容量不足的情况。成功修改内容会推进可观察的 `revision`；内容未变的替换或失败操作不会推进修订号。`read` 仅在回调执行期间借用文本，不发生拷贝；需要监听内容变化时请订阅 `revision`。文档本身允许存储多行文本，单行限制由具体的编辑器负责。在已准备好的容量内执行替换不会发生内存分配。

## 输入派发 {#input-delivery}

原生输入与模拟输入遵循完全一致的派发顺序。已识别的手势、其延迟命令、业务通知与响应式更新会在处理下一条原始输入前全部执行完毕。因此，同一输入批次中先 `Tap` 后 `Key` 时，必定先完成 `Tap` 的整条链路。回调中隐藏、移除或重绑定节点的操作会立即影响同批次后续输入。系统（System）与绘制仍按帧调度运行。

空间输入在派发前会先刷新过期的布局或变换后的命中几何。未改变的几何直接复用；待处理的绘制脏区保留给帧渲染器。若布局计算失败，将跳过空间输入派发并取消当前活动输入，而不会去命中过期的旧坐标。

`Key` 携带 `code`、`pressed`、`modifiers: Option<KeyModifiers>` 与 `repeat: Option<bool>`。修饰键结构暴露 `shift`、`control`、`alt` 与 `meta`。`Some(false)` 表示非自动重复按键；`None` 表示当前平台未提供该元数据。SDL 与 Web 同时提供修饰键与重复标志快照；WGPU 的修饰键在收到原生修饰键更新前保持未知，并在失去焦点或挂起时清空。Linux 使用键盘设备状态与原生重复标志。NuttX 键盘记录不提供修饰键与重复元数据。模拟输入则显式指定这些字段。

## 文本编辑 {#text-editing}

`TextCommitted` 沿着当前聚焦节点、其祖先节点直至 `App` 回退监听器派发一段完整的 UTF-8 文本片段。可通过 `event.text()` 读取内容。内联 `on TextCommitted`、闭包与 `#[event]` 回调共享这条路由。返回 `false` 允许默认的 `TextEditing` 处理器写入该片段；消费该事件则会阻止默认编辑。

可通过 `TextCommitted::new("你好").into()` 构造模拟或自定义输入，也可将已有的 `String` 移入 `TextCommitted::new(text)`。静态字符串采用借用，自有字符串直接转移所有权。回调执行与延迟编辑在当前调用结束前借用该事件；若业务层需要长期保留文本，应自行拷贝存储。克隆持有所有权的事件文本会触发复制。平台文本编码转换与输入队列增长独立于预分配的回调及编辑存储。

SDL 将每次原生文本事件保留为一个独立片段；WGPU 保留每次文本或输入法提交片段，而不会拆散为单个字符。原生片段不保证对应完整的输入法组合会话或扩展字素簇（grapheme cluster）。Web 键盘源在输入法组合（composing）期间将按键交由输入法处理（包括回车、删除与方向键），既不取消浏览器默认行为，也不向编辑器或快捷键派发。组合生命周期与候选词窗口由平台各自管理，不由键盘源直接提供。

`TextInput` 仅保存外观配置。当节点上未显式提供编辑能力时，`TextInput` 的挂载钩子会自动安装 `TextEditing`。请同时注册这两个视图，或直接调用 `App::with_default_widgets`。`TextEditing` 持有文档绑定、有向选区、焦点显示状态及光标闪烁动画。`TextInput` 在绘制时直接读取这些组件与已准备好的排版结果，不会维护第二份文本副本。

通过 `TextEditing::read` 读取内容，通过 `selection()` 与 `cursor()` 获取位置。所有长度与位置均为 UTF-8 字节偏移。`set_selection(TextSelection { anchor, cursor })` 以原子方式校验两端点，并保留反向选区（`anchor > cursor`）。默认内联容量为 32 字节。可通过 `TextInput(capacity: 256)`、`TextInput::build().capacity(256)?` 或 `TextEditing::default().with_capacity(256)?` 在输入前准备更大的缓冲区。显式挂载的 `TextEditing` 会替代构造器默认创建的编辑组件。

若需从应用逻辑修改文本，请绑定 `TextDocument` 并调用文档方法。修改会广播给所有绑定该文档的编辑器，而各编辑器的选区保持独立。输入通知仅汇报用户交互触发的编辑，不对程序化文档修改触发通知。

```rust,ignore
#[compose]
fn search(document: model!(TextDocument)) {
    ui! {
        TextInput(width: 240, height: 36) [
            TextEditing::new(document),
            Placeholder("Search"),
        ] on TextEditing::Submitted {
            // 处理已接受的回车提交。
        }
    };
}

let document = app.add_model(TextDocument::new()).finish();
document.set_capacity(256)?;
document.set_text("Hello Mirui")?;
app.compose(root, search(document.clone()));
document.replace(6..11, "world")?;
```

提交的文本会作为单次原子操作替换当前选区。一段被接受的文本片段最多触发一次 `TextEditing::Changed { len: u32 }`；非法或超出容量的片段会被完整拒绝并记录警告。`Backspace` 与 `Delete` 优先删除非空选区；若选区为空，则每次删除一个 Unicode 标量值（scalar value）。`Left` 与 `Right` 在有选区时将其折叠到起点或终点，无选区时按单个 Unicode 标量值移动，而非按完整字素簇移动。`Changed` 仅汇报被输入处理器接受的内容变更，单纯的光标移动不会触发。直接调用组件方法只会更新组件状态，不会合成输入通知。

`Shift+Left`、`Shift+Right`、`Shift+Home` 与 `Shift+End` 固定选区锚点（anchor）并移动游标端以扩展选区。无 Shift 修饰时，方向键正常移动光标或折叠选区。在边界处原地踏步不会将输入框标脏，也不会发送通知。

点击（Tap）编辑器会将光标移至最近的已排版视觉位置并折叠选区。命中测试共享文本的水平滚动偏移与祖先变换。若文本排版缺失或已过期，点击不会改动选区；指针处理阶段不会临时执行文本排版。光标位置改变会发出 `SelectionChanged`，而非 `Changed`。

聚焦状态下，`TextInput` 使用焦点边框颜色在文本下方绘制选区背景。选区几何直接复用已准备好的视觉字形段（visual runs），并共享文本的滚动偏移、几何变换与裁剪区域；占位符文本不可被选中。

当输入处理器改变了选区时，`TextEditing::SelectionChanged { anchor: u32, cursor: u32 }` 会汇报更新后的有向 UTF-8 字节偏移。同时改变内容与选区的编辑会先发出 `Changed`。用完全相同的文本替换当前选区可能会折叠选区而不触发 `Changed`。被拒绝或未产生实际变化的操作不会发出通知。

`TextEditing::Submitted` 汇报被默认按键处理器接受的回车键按下事件（包含长按自动重复）。它不会插入换行符、清空内容、移动选区或改变焦点。业务层编写的按键处理器可以在默认处理器之前拦截并消费回车键。这两类通知均支持内联块、闭包、强类型回调与控件构建器。没有监听器的通知会直接跳过；通知入队失败不会回滚已完成的编辑，也不会在后续重放。

### 自定义编辑器示例 {#custom-editor-examples}

关于完整的自定义 `Component`、挂载回调、嵌套文档 `Model` 以及 `View` 注册示例，请参阅[组装文本编辑器](text-editing.md)。

Gallery 中的 `editor_lab` 包含 `SearchEditor`、`CommandEditor` 与 `SharedEditor`。每个编辑器都是一个带无绘制 `View` 的组件，在 `attach` 回调中向同一节点插入 `TextEditing` 并复用 `TextInput` 绘制外观。搜索框通过观察文档修订号过滤固定目录；命令框处理 `TextEditing::Submitted`；两个共享编辑器绑定同一个自有 `TextDocument`，并各自保留独立选区。源码拆分为[组件实现](../../src/gallery/demos/editor_lab/editors.rs)、[状态模型](../../src/gallery/demos/editor_lab/model.rs)与[界面组合](../../src/gallery/demos/editor_lab/composition.rs)三个模块。

## 指针身份 {#pointer-identity}

原始 `InputEvent` 指针记录携带 `kind: PointerKind`（`Mouse`、`Touch`、`Pen` 或 `Unknown`）以及 `id`。手势识别与滚动捕获会同时匹配这两个字段；来自其他指针的移动或释放不会终止当前捕获。各后端直接上报设备类别，而不是根据槽位编号猜测。模拟光标操作使用 `Mouse`；模拟双指缩放与旋转触点使用 `Touch`。

`PointerEnter` 与 `PointerLeave` 仅汇报表面边界进出，不会凭空捏造坐标。进入表面后会等待下一条带坐标的输入才执行命中测试。离开表面会清除悬停（hover）状态与光标反馈，但不会释放正在进行的拖拽；已捕获的指针移动仍使用其实时坐标。触摸输入不产生悬停反馈，手写笔悬停则需要显式的进入事件或非接触移动。

Web 与 WGPU 将活动的原生指针映射到稳定的槽位：鼠标固定使用槽位 `0`，其余最多 16 个指针占用独立槽位。映射表满时会忽略新触点，而不会顶替活动触点。触摸槽位在释放或取消时回收；Web 悬停指针在未按下时保留槽位，直至离开表面。宿主窗口失去焦点或不可见时会清空映射表。Web 丢失指针捕获（lostpointercapture）会取消活动按压；紧随正常释放之后的捕获丢失事件不会重复触发取消。指针槽位编号不是持久化的硬件设备 ID。

## 节点悬停 {#node-hover}

`HoverChanged { hovered, x, y }` 汇报鼠标或悬停手写笔的命中路径是否包含当前节点。支持内联代码块、闭包与 `#[event]` 回调：

```rust,ignore
#[event]
fn hovered(event: event!(HoverChanged), state: model!(CardState)) {
    state.set_hovered(event.hovered);
}

Column() {
    Button("Open") on Tap { open(); }
} on HoverChanged(hovered::callback(state))
```

订阅悬停通知不会让原本不可交互的节点变成可命中目标，也不会自动添加视觉反馈。容器节点通过其内部已有的可交互后代节点感知悬停。触摸与未知类型的指针不产生悬停。多个悬停指针共享命中集合；只有当所有悬停指针都离开该节点时才会触发离开通知。按下按键不会清除悬停，离开表面也不会中断拖拽捕获。

离开通知按从子节点到父节点的顺序派发，随后进入通知按从父节点到子节点的顺序派发。在同一个父容器的不同子节点之间移动时，共同祖先节点不会反复切换悬停状态。事件坐标为逻辑表面坐标，而非节点局部坐标。隐藏、禁用、移除或重绑定的节点会立即失去悬停资格；已发布的布局变更会自动更新静止指针的悬停归属。新注册的监听器不会重放历史跃迁。悬停通知共享 `EventCapacity`；入队失败会记录日志并直接丢弃，不会重试。

## 按压生命周期 {#press-lifecycle}

`Pressed { pointer_id, x, y }` 标志单指针按压的开始。`Released` 在正常抬起时携带相同字段（即使指针已移出节点初始边界）；收到 `Released` 并不意味着一定触发 `Tap`。`PressCancelled` 额外包含 `reason: PressCancelReason`，取值为 `Scroll`、`Gesture`、`Platform`、`Suspended` 或 `Unavailable`。

```rust,ignore
Button("Open")
    on Pressed { state.set_pressed(true); }
    on Released { state.set_pressed(false); }
    on PressCancelled { state.set_pressed(false); }
```

这三种通知同样支持闭包与 `#[event]` 回调。它们不消费输入、不增加命中目标，也不自动开启视觉反馈。坐标为逻辑表面坐标；`pointer_id` 是后端触点槽位而非持久化设备 ID。捕获机制在内部同时区分指针类别与槽位。

初始按下时会捕获命中的目标节点及其监听了按压通知的祖先链，并按从子到父的顺序发出通知。后续指针移动、移出表面、修改父节点或新增监听器都不会改变这条已捕获的路径。长按（LongPress）继续保留该捕获。滚动接管、拖拽或多指手势识别成立、平台取消、应用挂起以及目标失效都会将按压转为 `PressCancelled`，而不是触发正常释放。被移除或重绑定的接收方不会收到旧身份的通知。每次按压最多只会终止一次；若通知队列已满，终止通知会被丢弃，既不重试也不影响手势识别状态。

## 滚动几何 {#scroll-geometry}

`mirui::input::event::scroll::ScrollBounds` 是由布局阶段生成的组件，包含 `viewport_width`、`viewport_height`、`content_width`、`content_height`、`max_x` 与 `max_y`。可在事件、系统或视图中通过 `query!(ScrollBounds)` 读取。在首次成功完成布局前该组件不存在；配置修改会在下一次布局成功后反映到该快照中。若某次布局失败，仍保留上一次的有效值。

```rust,ignore
#[event]
fn inspect_scroll(node: entity!(), bounds: query!(ScrollBounds), state: model!(ScrollReadout)) {
    if let Some(bounds) = bounds.get(node) {
        state.set_max_offset(bounds.max_y);
    }
}
```

请通过 `ScrollConfig` 而非 `ScrollBounds` 配置可滚动内容。保留的后代节点、裁剪设置与显式尺寸共同决定可滚动范围；`LazyList` 使用全部逻辑项的总跨度，而非当前可见单元格池的大小。当节点移出界面树或不再持有 `ScrollOffset` 时，下一次成功布局会自动移除已发布的 `ScrollBounds`。

## 滚动通知 {#scroll-notifications}

`ScrollStarted { offset }` 标记首次发生实际位移。`ScrollChanged { old, new }` 汇报内容偏移量（包含亚像素小数位置与弹性越界位置）。`ScrollEnded { offset, reason }` 在输入、惯性滑行与边界回弹全部结束后触发；`reason` 为 `ScrollEndReason::Settled` 或 `Cancelled`。只要内容仍在惯性移动，单纯抬起手指或鼠标不会立即结束当前滚动段。

```rust,ignore
#[event]
fn track_scroll(change: event!(ScrollChanged), readout: model!(ScrollReadout)) {
    readout.set_position(change.new.y);
}

#[compose]
fn panel(readout: model!(ScrollReadout)) {
    ui! {
        Column(height: 240, clip_children: true) [
            ScrollOffset::default(), ScrollConfig::default(),
        ] on ScrollChanged(track_scroll::callback(readout)) {
            Text("Content", height: 480)
        }
    };
}
```

内联代码块与闭包同样可订阅这组通知。目标节点必须持有 `ScrollOffset`；订阅本身不会增加命中目标、消费输入或添加视觉反馈。鼠标滚轮、触摸拖拽、旋钮（rotary）以及回调中的程序化写入在回调结束后共用同一套观察机制。在没有活动滚动时执行一次离散写入，会依次产生 `Started`、`Changed` 与 `Ended`；若在同一个回调内将偏移量改回初始值，则不会产生任何通知。

切换滚动目标或取消输入会以 `Cancelled` 结束旧的滚动段。被移除、隐藏、脱离树或重绑定的数据源会丢弃旧观察状态。新注册的监听器不会重放此前的滚动位移；通知预算耗尽时会丢弃本次通知，不会重试或回滚滚动位置。

## 节点焦点 {#node-focus}

将 `PointerFocus::Preserve` 挂载到编辑工具栏或屏幕软键盘上，即可在点击工具栏按钮时保留原有编辑器的键盘焦点。命中节点或其祖先链上最近的显式策略生效；`PointerFocus::Automatic` 可覆盖外层祖先的保留策略。未声明策略时，指针激活会聚焦最近的可聚焦节点，若无则清空焦点。保留策略不会恢复已隐藏、已移除或已被其他方式清除的目标，也不会干预键盘导航或程序化焦点切换。

```rust,ignore
Row() [PointerFocus::Preserve] {
    Button() [Text::label("Replace")] on Tap(replace_document::callback(document.clone()))
}
```

`FocusChanged { focused, related }` 是直接发往节点的通知。`related` 标识本次焦点转移涉及的另一个节点（读取时该节点可能已销毁）。可通过内联块、闭包或 `#[event]` 回调订阅：

```rust,ignore
#[event]
fn focus_changed(event: event!(FocusChanged), state: model!(EditorState)) {
    state.set_active(event.focused);
}

TextInput() on FocusChanged(focus_changed::callback(state))
```

`FocusState.focused` 依然是全局焦点资源。单次回调内的多次写入只发布最终焦点值；不同的 `Event`、`System` 与 `Timer` 回调则各自保留独立的焦点跃迁。旧节点先收到失焦通知，新节点后收到获焦通知。点击（Tap）会先切换焦点并派发焦点通知，随后才运行业务层编写的 `Tap` 处理器。焦点监听器内部发起的焦点修改会排队延后处理，不会递归重入。隐藏、禁用、移除或重绑定的目标会失去焦点资格；发往已销毁或已退役身份的通知会被直接丢弃。注册监听器不会重放当前已有的焦点状态，也不会增加指针反馈。宿主窗口焦点与控件焦点相互独立。

焦点通知共享 `EventCapacity` 存储及其单次刷新上限。入队失败会记录日志并丢弃通知，既不回滚焦点也不重试。

## 表面变更 {#surface-changes}

通过 `App::on_event` 注册 `ViewportChanged` 与 `SafeAreaChanged`。两者均携带 `old` 与 `new` 值。即使逻辑宽高不变，只要物理像素尺寸或缩放因子（scale）改变，就会触发 `ViewportChanged`；安全区域内边距（safe-area insets）以逻辑像素为单位。

```rust,ignore
app.on_event(ViewportChanged::EVENT, resized::callback());
```

表面资源会在执行这些回调以及派发后续输入之前先行更新。此时节点布局可能尚未重算；若需观察节点最终解析出的尺寸，请使用 `SizeChanged`。初始状态与未发生变化的数值不会发出通知，晚注册的订阅者也不会收到历史重放。App 级事件载荷存储在安装监听器时即完成准备，独立于业务通知预算。

## 宿主状态 {#host-state}

`HostFocusChanged { focused }` 与 `HostVisibilityChanged { visible }` 是 App 级通知。首次平台汇报会确立初始状态；重复的相同数值会被忽略，晚注册的监听器不会收到重放。不提供相关汇报的平台将该状态保持为未知。

SDL 汇报窗口焦点及显示/最小化状态。WGPU 汇报窗口焦点与遮挡（occlusion）状态。Web 在文档处于聚焦状态时汇报画布（canvas）焦点，并汇报文档可见性；它不会推断 CSS 可见性或上方覆盖的 DOM 元素。

失去宿主焦点或可见性会在派发通知前先行取消当前活动输入，但保留控件自身的逻辑焦点。这两类通知都不会挂起或恢复 `App`；`AutoSuspendPlugin` 仅响应显式的平台挂起/恢复请求。

## 初始化 {#initialization}

在控件组装阶段，使用 `attach = initialize::callback()` 声明初始化钩子。该回调使用 `#[event]` 宏，支持与其他事件回调相同的 `entity!()`、`query!(...)`、资源、模型及命令参数，但不需要声明事件载荷参数。

```rust,ignore
#[event]
fn initialize(node: entity!(), commands: commands!()) {
    commands.insert(node, TouchAction::PanY);
}

#[view(component = Slider, attach = initialize::callback())]
fn slider_behavior() {}
```

静态组件、父子关系链接与节点 ID 会在初始化回调运行前安装完毕。回调借用结束且延迟命令提交后，才会执行下一个组件的挂载初始化。响应式属性、业务层事件绑定以及子节点按正常组装顺序处理。初始化过程不冒泡、不添加指针反馈，也不占用业务通知存储。

每个已注册的 `View` 在遇到第一个匹配实体时准备其初始化器，并在同一 `App` 的后续实例间复用。每次组件挂载只运行一次初始化：普通字段写入保留原有挂载身份，而替换组件或移除后重新插入则会视为一次新挂载。初始化器内部插入的其他组件无论 `View` 注册先后顺序如何，都会触发各自已注册的初始化逻辑。新注册的 `View` 也会在下一次输入或渲染边界处为已组装的存量控件补做初始化。

UI 派发管线在冻结每条输入或通知路由前，会先完成所有待处理的挂载初始化。因此，回调可以为下一次事件动态安装组件，而不会把新组件的输入钩子混入当前正在派发的事件中。当没有待处理输入时，渲染准备阶段也会执行待定的初始化。

请在构建控件前配置好事件容量。初始化准备失败会记录警告并禁用该初始化器；执行期间失败会使该实例无法响应 `View` 输入，但不会回滚已完成的修改或自动重试。重复的结构性反馈受固定上限约束并记录日志。槽位（niche）声明保留各自的组合生命周期与槽位构建流程。

## 输入 {#input}

使用 `#[view(input = ...)]` 声明控件的默认输入行为。每个处理器复用现有的 `#[event]` 参数体系；`entity!()` 标识当前控件节点，手势载荷则保留初始命中目标。

```rust,ignore
#[event]
fn toggle(
    node: entity!(),
    _tap: event!(Tap),
    boxes: query!(Checkbox),
    commands: commands!(),
) -> bool {
    let Some(checkbox) = boxes.get(node) else {
        return false;
    };
    let now = !checkbox.checked;
    commands.set::<mirui::ui::property::prop::CheckboxChecked>(node, now);
    commands.emit(node, mirui::ui::widgets::checkbox::Toggled { now });
    true
}

#[view(component = Checkbox, input = toggle::callback())]
fn checkbox_input() {}
```

通过 `App::with_widget` 注册该描述符。只有当默认描述符未被同时注册时，针对已有组件编写的自定义描述符才会接管其默认输入实现。空函数体表示不提供绘制逻辑；输入处理与绘制可以合并在同一个描述符中。

若有多个输入回调，请传入元组：`input = (tap::callback(), drag::callback(), key::callback())`。处理器通过 `event!(Tap)`、`event!(DragMove)`、`event!(Key)` 等声明所需载荷类型。支持的手势包括 `Tap`、`LongPress`、`DragStart`、`DragMove`、`DragEnd`、`DragCancel`、`Pinch` 与 `Rotate`；键盘输入包括 `Key` 与 `TextCommitted`。业务通知仍通过 `on` 绑定编写，不属于 `View` 输入钩子。

回调在配置好 `App` 事件容量后、首次组装匹配控件时完成准备。同一视图的匹配实例共享预置的回调存储，不会为每个实例单独分配命令队列。每次调用都会传入当前实体并重新借用参数，稳定的输入派发直接复用这套预分配存储。

手势钩子在同一节点上早于业务层 `on` 监听器运行；键盘钩子则在聚焦节点及其祖先的业务层监听器之后、`App` 回退监听器之前运行。同一个 `View` 内部匹配的多个钩子按元组声明顺序依次执行。返回 `true` 或 `()` 表示消费该输入；返回 `false` 则继续沿路由传递。仅包含键盘钩子的视图不会增加指针命中目标或交互反馈。实体生命周期与保留行（retained-cell）守卫同样作用于冻结后的路由。此外，`View` 钩子要求组件挂载已完成初始化且未被替换；中途替换或移除组件会使剩余的已冻结钩子失效（即使在派发恢复前新组件已完成初始化）。

## 通知派发 {#notification-delivery}

`Slider` 会汇报 `DragStarted`、`ValueChanged`，并以 `DragEnded` 或 `DragCancelled` 收尾。取消拖拽会保留最后一次有效数值，且不会再额外汇报正常结束。

`commands.emit(node, payload)` 记录一条延迟通知命令。在命令提交时，目标实体上必须存在匹配该载荷类型的监听器，否则载荷会被直接丢弃。被跳过的事件不占用业务通知存储，后续新增的订阅者也不会收到它们。但在记录命令当下，载荷表达式仍会正常求值。

`commands.emit_always(node, payload)` 记录一条无条件入队请求。即使监听器是在入队之后、正式派发之前安装的，也能收到这条通知。若派发时仍无监听器，事件才会被丢弃；队列不保存历史记录。无效实体、缺失源组件或通知容量不足仍会走常规回调错误路径。

这两种方法均使用 `CommandCapacity::emit::<Payload>(count)` 预留命令记录与载荷存储。真正入队的通知还会消耗 `App` 的业务通知预算。记录或提交阶段的错误由统一执行器处理，已完成的模型或组件修改不会回滚。命令在回调借用结束后按记录顺序执行，因此在发出通知前记录的属性修改，一定会在该通知派发前生效。

排队中的通知会绑定源单元格的数据身份（data-binding identity）。当虚拟列表单元格被重绑定或停用时，归属旧身份的待处理通知（包括正在派发但尚未读完的回调）会被一并丢弃。新通知使用当前绑定身份。被丢弃的载荷直接释放，不会重放。

显式配置的 `EventCapacity` 会替换 `App` 的默认预设。其命令预算同样约束已注册控件的初始化回调与输入回调（包括插入组件、修改属性、发起动画及发送通知），而非仅作用于业务层手写的 `on` 处理器。

原生与软件输入法会话会将 `CompositionStarted`、`CompositionUpdated` 与 `CompositionEnded` 直接派发给发起会话的编辑器节点。这些回调返回 `()`、不冒泡，也不能作为普通业务通知手动发出。编辑器的 `View` 输入钩子先于业务层监听器运行。`CompositionUpdated::text()` 借用预编辑文本，`selection()` 返回可选的 UTF-8 字节范围。`CompositionEnded::reason` 区分 `Committed`、`Cancelled` 与 `Interrupted`。

`TextCommitted` 依然可被消费：在拦截器中拒绝它会阻止默认文本插入，但仍会正常结束本次组合会话。提交过程会先清除预编辑预览，再走拦截与文档/选区通知流程，最后汇报 `CompositionEnded`。`Committed` 表示平台已交付该文本片段，并不代表编辑器最终成功插入了文本。切换焦点、替换编辑器或文档、重绑定虚拟单元格或从外部修改正在组合的文档，都会使旧会话的请求失效。

如果晚注册的订阅者需要获取当前状态，请使用 `Model` 订阅。通知仅描述某次事件的发生；无论哪种发送方式都不会为未来的订阅者留存历史事件。
