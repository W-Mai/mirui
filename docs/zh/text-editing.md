# 组装文本编辑器 {#composing-text-editors}

`TextEditing` 通过[剪贴板命令](clipboard.md)提供编辑器局部的文本选择、复制、带确认剪切以及选区感知粘贴能力。当多个编辑器共享同一份文档时，各编辑器各自保留独立的选区。

`TextDocument` 负责存储 UTF-8 文本内容及其字节容量。`TextEditing` 将该文档绑定到单个编辑器的选区、焦点及输入处理流程上。`TextInput` 则负责绘制编辑器外观。自定义组件只需在同一实体上组合这几项能力，即可直接获得完整的编辑体验，而无需直接访问 `World` 或自行编写键盘处理逻辑。

<div class="arch-diagram" role="img" aria-label="从共享 TextDocument 经由 TextEditing 与 TextInput 到原生或软件输入法会话的文本编辑器组合架构">
  <svg viewBox="0 0 940 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="edit-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="30" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">01 · DOCUMENT MODEL</text>
    <text x="30" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">共享 UTF-8 存储</text>
    <text x="30" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextDocument · revision()</text>
    <text x="30" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">set_capacity · replace</text>
    <text x="30" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">独立于编辑器视图外观</text>
    <path d="M224 84H244" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="250" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">02 · EDITING STATE</text>
    <text x="264" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">选区与输入处理</text>
    <text x="264" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextEditing · TextSelection</text>
    <text x="264" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">Changed · Submitted</text>
    <text x="264" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">各编辑器独立光标与预编辑</text>
    <path d="M458 84H478" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="484" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="498" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">03 · APPEARANCE</text>
    <text x="498" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">视觉字形段与光标</text>
    <text x="498" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextInput · Placeholder</text>
    <text x="498" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">attach = callback()</text>
    <text x="498" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">零冗余文本副本</text>
    <path d="M692 84H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="718" y="20" width="206" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="732" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">04 · IME BRIDGE</text>
    <text x="732" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">会话与按键</text>
    <text x="732" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextInputSession</text>
    <text x="732" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">preedit · commit</text>
    <text x="732" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">原生 / 软件输入法</text>
  </svg>
</div>

在编辑器内按下并拖拽会将选区锚点（anchor）固定在起始光标位置，并随指针移动扩展另一端点。松开指针或取消手势都会保留已选中的范围。若拖拽途中编辑器失去焦点，或其文本内容、选区被外部修改，拖拽选区会立即中止。除非节点上已显式指定触摸策略，否则编辑器默认挂载 `TouchAction::PanY`，从而将水平拖拽留给文本选区，同时把垂直拖拽交还给外层可滚动祖先容器。

调用 `commands.selection().select_all(editor)` 即可全选目标编辑器的整篇文档，且不会改变当前的键盘焦点。其中 `editor` 是目标节点实体；`TextEditing` 为自身（包括挂载了 `TextEditing` 的自定义编辑器）提供全选实现。共享同一文档的其他编辑器仍保留各自独立的选区。工具栏容器可以挂载 `PointerFocus::Preserve`，以免点击工具栏按钮时夺走编辑器的焦点。选区变更依然通过现有的 `SelectionChanged` 通知对外广播。

## 自定义搜索编辑器 {#a-custom-search-editor}

下面的示例将查询文本保存在应用层的 `Model` 中。`SearchEditor()` 在挂载时自动补齐编辑能力与标准输入框外观；第二个 `TextInput()` 共享同一份查询文档，同时拥有各自独立的选区。按下回车键时，通过现有的通知 API 调用模型的提交方法。

<MiruiSnippet recipe="editor-shared" label="TextDocument & SearchEditor" file="recipes.rs" height="210px">

<template #exercise>

在任意一个输入框中键入文字（或点击 `Set 'SPI'`）：上下两个输入框共享同一个 `TextDocument`，但各自保留独立的光标与选区。按回车或点击 `Submit` 递增提交计数。

</template>

</MiruiSnippet>

注册 `View` 用于安装一类控件的行为逻辑，而在节点上插入 `Component` 则是提供单节点的实例状态——这两者是彼此独立的操作。`App::with_default_widgets()` 已经预先注册了 `TextInput` 与 `TextEditing`，因此使用默认预设的应用只需额外注册自定义的 `search_view::view()`。上面空的 `search_view` 函数体表示该视图自身不执行绘制；它的 `attach` 钩子在插入默认组件前会先检查节点是否已经显式携带了 `TextEditing` 或 `TextInput`，避免覆盖调用方传入的定制配置。

未传入 `document` 的 `SearchEditor()` 会创建一个 32 字节内联容量的本地文档。若需要更大的本地缓冲区，可在组装该节点时显式挂载 `TextEditing::default().with_capacity(256)?`。若使用的是共享文档，请在组装各编辑器之前，直接在已注册的 `TextDocument` 上调用 `set_capacity` 准备好容量。容量限制按 UTF-8 字节数（而非字符数）计算；被拒绝的越界编辑不会改动原有内容。扩容操作可能会分配内存，而在已准备好的容量范围内替换文本则会直接复用现有缓冲区。

程序化修改通过 `form.query()` 直接作用于文档并通知其观察者，不会伪造 `Changed` 或 `Submitted` 等用户输入通知。每个编辑器各自维护一套 UTF-8 选区；当外部修改导致原选区越界时，选区端点会被钳制到合法边界，而不会按编辑历史做相对平移。若需根据查询文本派生业务状态，请订阅文档的 `revision()` 并使用 `read` 借用文本。

`SearchEditor` 上的 `document` 字段仅在组件首次挂载（attach）时提供初始文档绑定。后续单纯修改该字段并不会让已经挂载的 `TextEditing` 重新绑定到新文档；若需切换现有编辑器的数据源，请通过 `commands.insert` 直接替换节点上的 `TextEditing` 组件。

## 软件输入法 {#software-input-methods}

`InputMethod::software` 在每个 `App` 中一次性安装一个 `TextInputChanged` 回调以及可选的 `Key` 回调。当存在活动编辑器时，`TextInputChanged::state` 包含当前聚焦编辑器的会话标识（`session`）与光标几何信息。会话标识改变意味着旧的候选词已失效；仅光标几何位置发生变化时，会话标识保持不变。若 `state` 为 `None`，则表示当前没有活动的文本编辑器。

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text, TextInput};

#[model]
#[derive(Default)]
struct Keyboard { session: Option<TextInputSession> }

#[model]
impl Keyboard {
    fn session(&self) -> Option<TextInputSession> { self.session }
    fn set_session(&mut self, session: Option<TextInputSession>) { self.session = session; }
}

#[event]
fn context(event: event!(TextInputChanged), keyboard: model!(Keyboard)) {
    keyboard.set_session(event.state.map(|state| state.session));
}

#[compose]
fn keyboard_panel(keyboard: model!(Keyboard)) {
    ui! {
        Column(row_gap: 8) {
            TextInput(width: 240, height: 36, capacity: 128, preedit_capacity: 256)
            Row(height: 36, column_gap: 8) [PointerFocus::Preserve] {
                Button(width: 80) [Text::label("ni")]
                    on Tap {
                        if let Some(session) = keyboard.session() {
                            commands!().text_input().preedit(session, "ni", Some(2..2));
                        }
                    }
                Button(width: 80) [Text::label("你")]
                    on Tap {
                        if let Some(session) = keyboard.session() {
                            commands!().text_input().commit(session, "你");
                        }
                    }
            }
        }
    };
}

# fn main() {
let mut app = App::headless(320, 160);
app.with_default_widgets();
let keyboard = app.add_model(Keyboard::default()).finish();
app.with_input_method(InputMethod::software((context::callback(keyboard.clone()),)));
let root = app.spawn_root().id();
app.compose(root, keyboard_panel(keyboard));
# }
```

先点击输入框使其获焦，随后点击下方的软键盘按钮。`preedit` 仅在目标编辑器的显示画面中用预编辑预览替换当前选区；共享同一文档的其他编辑器仍显示已提交的正式内容。`commit` 会将 `TextCommitted` 送入目标编辑器常规的拦截与编辑路由。`cancel` 会清除预编辑预览而不修改文档。`key` 则向目标编辑器直接发送编辑按键，且不会再次重入软件输入法自身的按键回调。

`commands.text_input().set_mode(TextInputMode::Native)` 切换为平台原生输入法；`Software` 则切换为已安装的软件输入法提供方。切换模式会立即取消当前正在进行的组合输入并使其会话失效。处于 `Software` 模式时，平台原生提交的文本会被忽略。软件输入法可选的 `Key` 回调会在编辑器默认处理之前先行拦截受支持的编辑键；返回 `false` 则放行给编辑器正常处理。该回调仅用于拦截编辑控制键，不负责原始字符转译或键盘布局映射。

`capacity` 限制已提交正文的 UTF-8 字节数；`preedit_capacity` 限制未提交预编辑片段的字节数（默认为 `capacity` 的两倍，显式指定时二者相互独立）。显示缓冲区在组件挂载或显式调整容量时完成准备，并在后续更新中反复复用。超出预编辑容量的预览会被直接清空（而非截断），后续长度合法的预览仍可正常显示。静态字符串片段直接借用；已有的 `String` 可移入请求中。动态拼接候选词文本由输入源自行负责。

输入法命令会在回调借用全部释放后统一派发。默认事件预设已包含文本输入命令容量；若使用自定义容量配置，请设置 `CommandCapacity::text_input(count)` 并确保单次刷新上限非零。软件输入法请求队列根据配置的命令记录数预先分配。发往已失效会话的请求会被直接丢弃；当队列容量耗尽时，会记录一条警告并丢弃新请求，不会重放或回滚。

关于通知的先后顺序，请参阅[控件初始化、输入与通知](widget-events.md)。Gallery 中的 [Editor Lab](../../src/gallery/demos/editor_lab) 完整展示了搜索过滤、命令提交、共享文档双栏编辑，以及带有 `n → ni → 你` 静态拼音序列的屏上软键盘。

使用 SDL 或 WGPU 后端运行 Editor Lab，并在界面中切换到 **System** 即可体验操作系统原生输入法：

```bash
cargo run -p gallery --example editor_lab_demo --no-default-features --features sdl
cargo run -p gallery --example editor_lab_demo --no-default-features --features wgpu
```

## 原生输入后端 {#native-input-backends}

`App` 会在轮询输入前以及完成布局与渲染后调用 `Surface::set_text_input(Option<TextInputState>)`。传入 `Some` 会激活一个原生编辑器会话并上报其在逻辑表面坐标系下的光标矩形；传入 `None` 则停止原生文本输入。单纯移动光标不会替换当前会话标识。不具备原生文本输入服务的表面可以直接使用默认的空实现。

各后端在接收原生输入时捕获当时的 `state.session`，并从 `poll_event` 返回 `session.preedit(text, selection)`、`session.commit(text)` 或 `session.cancel()`。预编辑内部选区同样使用 UTF-8 字节偏移。这些构造器可接收自有 `String` 或借用静态字符串；事件派发期间仅借用载荷。归属已退役会话的事件会被直接丢弃，绝不会张冠李戴地转发给后来新获焦的编辑器。

SDL 软件表面与 SDL GPU 表面共用这套协议。当会话切换时，适配器会先清除平台组合状态并排空队列中残留的文本事件，随后再重启原生输入法；仅更新光标矩形不会重启输入法。空预编辑预览会保留在当前组合状态中，直到队列中的提交事件到达或 SDL 事件队列变为空闲。由于 SDL 不会为后续的操作系统回调绑定编辑器身份，对于尚未被适配器捕获的在途输入，适配器依赖平台侧的取消机制进行清理。此外，SDL 绑定暴露的是定长原生文本事件，而非扩展长预编辑事件。

WGPU 表面在 Winit 开启输入法组合（composition）时捕获当前编辑器会话。后续的预编辑、提交与关闭事件均保留该捕获时的会话标识；中途切换焦点不会篡改已捕获输入的会话归属。普通键盘字符输入同样携带捕获瞬间的会话标识。如果原生事件批次结束时预编辑变为空串且同一批次内没有紧随其后的提交事件，适配器会在批次末尾自动取消该次组合。`MobileSurface::finish_input_batch` 负责将这一批次边界与移动端平台事件循环衔接起来。

Winit 0.30 在桌面端提供了完整的预编辑事件与候选窗光标定位支持；但其 iOS 与 Android 实现目前仅暴露键盘字符输入与软键盘显隐控制，尚未转发预编辑事件，也不支持光标区域 API。因此，这两个移动端后端上的原生输入法组合体验受限于底层平台适配器。而在应用内自建的软件输入法在所有后端路径上均使用完全相同的编辑器会话机制。

Web Canvas 为每个原生编辑器会话创建一个透明的 DOM `<textarea>`。浏览器 `compositionupdate` 事件实时更新预编辑预览；`compositionend` 提交或取消组合。普通 DOM 输入事件负责提交文本，而键盘事件仅传递编辑控制键，避免重复插入字符。DOM 的 UTF-16 选区偏移会自动转换为 UTF-8 字节边界。当会话被替换时，旧的 `<textarea>` 及其事件监听器会被立即移除；已捕获的在途事件仍保留其原始会话标识。

Web 运行器在浏览器事件冒泡阶段交付输入，并在 `requestAnimationFrame` 动画帧上执行渲染。Canvas 与其配套的 `<textarea>` 共享宿主焦点；在二者之间切换焦点不会取消当前编辑器会话。`<textarea>` 根据 Canvas 的 CSS 边界定位在逻辑光标处。浏览器自身的用户激活策略（user activation）依然决定移动端软键盘何时弹出。该轴对齐的候选框锚点不反映 CSS 旋转或斜切（skew）变换。

这个透明 DOM `<textarea>` 仅充当新输入文本的传输通道，不会在 DOM 中保留整篇文档的副本。因此，该传输层不提供原生上下文联想（surrounding-text prediction）、跨整篇文档的 DOM 原生选区或富格式剪贴板操作。创建会话时会分配对应的 DOM 节点与事件监听器；浏览器侧的字符串转换与 CSS 光标位置更新可能会产生分配，而 `Model`/`View` 状态更新与编辑器内部缓冲区复用则严格保持既有的内存契约。
