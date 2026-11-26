# 跨平台剪贴板 {#clipboard}

`commands.clipboard().copy(editor)` 复制指定实体所属的选区。`TextInput` 以及挂载了 `TextEditing` 的自定义控件会自动提供该能力。反向的 UTF-8 选区（游标在锚点之前）会被自动规范化为正向范围；若选区为空，平台剪贴板保持原样不变。复制操作不会改变焦点、选区、文档内容或正在进行的输入法预编辑（composition）。

```rust,ignore
Button () [PointerFocus::Preserve, Text::label("Copy")]
on Tap {
    commands!().clipboard().copy(editor);
}
```

剪贴板命令按入队顺序串行执行。先调用 `select_all(editor)` 再调用 `copy(editor)` 会复制全选后的全部文本；若把两个调用顺序反过来，则会复制全选之前的旧选区。`copy` 命令在返回前即捕获不可变的数据快照，因此后续对文档的修改不会篡改正在排队写入平台剪贴板的内容。

`commands.clipboard().cut(editor)` 捕获相同的选区快照，并**等待平台剪贴板确认写入成功之后**才从源编辑器中删除选区文本。空选区不执行任何操作。如果平台写入失败，文档内容、选区与预编辑状态均保持不变。如果平台写入尚在挂起等待期间，源实体、选区、数据提供者或输入法预编辑状态发生了变化（哪怕改完又改回原样），剪贴板仍会保留已捕获的文本，但编辑器中的内容将不再被删除。`TextEditing` 接受删除后会将光标折叠到被删范围的起点，在修改文档前退役旧的输入法会话，并在修改后发出预编辑结束通知。

`commands.clipboard().paste(editor)` 读取平台剪贴板数据并替换目标编辑器的当前选区（若无选区则在光标处插入），插入后光标移至新文本末尾。命令在发起平台读取前即锁定目标实体与当时的编辑上下文快照；如果在等待读取期间源实体、选区、提供者或预编辑状态发生变化，挂起的粘贴操作会被拒绝，绝不会把旧偏移量强行套用到新内容上。容量不足、格式不支持或文本校验失败都会原样保留现有文档、选区与预编辑状态。

默认快捷键绑定（`Ctrl/Command+A` 全选、`C` 复制、`X` 剪切、`V` 粘贴）作用于当前聚焦的控件。这些默认动作在按键回调允许继续冒泡之后执行；若 `Key` 回调已消费该按键，则不会触发默认剪贴板动作。目标实体与能力提供者在运行按键回调之前即已锁定，因此回调内临时转移焦点不会把剪贴板操作重定向到别的控件上。已移除、隐藏、替换或重绑定的目标会被拒绝。按键抬起（release）、长按自动重复（auto-repeat）以及包含 `Alt` / `Shift` 的组合键不会触发这些默认快捷键。在输入法预编辑期间按复制会读取正式文档中的选区而不提交或取消预编辑；剪切与粘贴则只在编辑被正式接受后打断预编辑会话。

软件输入法可以通过 `commands.text_input().key(session, key)` 发送相同的快捷键，直接复用会话所属编辑器的选区默认动作，而不会重入输入法自身的按键拦截器。

## 完成通知 (`ClipboardFinished`) {#completion}

请在**目标编辑器**（而非工具栏上的“复制/粘贴”按钮）上监听 `mirui::ui::clipboard::ClipboardFinished`。其中的操作 ID 唯一标识一次请求，`result` 字段区分以下结果：
- `Copied`：平台剪贴板写入已确认成功；
- `Cut`：平台写入成功，且源组件已接受并执行了选区删除；
- `CopiedWithoutCut`：平台写入成功，但源组件在等待期间变得不可用或拒绝了删除；
- `Pasted`：目标组件已校验并接受了导入的数据；
- `EmptySelection`：复制或剪切时的选区为空；
- `ClipboardError`：权限被拒、格式不支持、容量不足或平台底层失败。

完成通知绝不会自动重试失败的写入或编辑；重复确认或来自其他 `App` 实例的跨界确认会被直接忽略。

```rust,ignore
TextInput () on ClipboardFinished(|event: event!(ClipboardFinished)| {
    model.clipboard_finished(event.result);
})
```

常规的有界事件预设（event preset）已默认支持剪贴板完成通知。如需显式配置容量预算，可通过 `CommandCapacity::clipboard(count)` 与 `EventCapacity::notifications::<ClipboardFinished>(count)` 预留。当目标节点未安装 `ClipboardFinished` 监听器时，成功通知不会入队，但失败仍会输出日志。

完成通知严格绑定到发起操作时的实体与列表行身份（retained item binding）：已被删除的目标或已被复用到其他数据行的虚拟化单元格不会收到旧条目的剪贴板结果。已捕获快照的 `copy` 在原条目被移除后仍可完成向系统剪贴板的写入；而挂起的 `cut` 与 `paste` 则绝不会误改新换上的条目。

## 自定义可选中与可粘贴组件 {#custom-selectable-components}

自定义组件可以通过现有的 `#[event]` 回调与 `View` 的 `input` 列表（或节点上的显式 `on` 绑定）注册 `InspectSelection` 与 `CopySelection`。它们是同步的单提供者请求（request），而非冒泡手势或异步队列通知：

- **`InspectSelection`**：返回 `Result<SelectionState, ClipboardError>`。使用 `SelectionState::new(has_selection, context)` 构造状态，其中 `context: Eq + 'static` 持有源身份以及内容/选区修订号的值快照（哪怕内容修改后又改回原值，修订号也应当递增；切勿在其中存放动态查询闭包）。
- **`CopySelection`**：返回 `Result<ClipboardData, ClipboardError>`，且绝不能修改源组件。框架会在导出前后比对选区上下文，并独立校验节点、提供者、组件挂载状态与列表行身份。
- **`CutSelection`**（可选，支持剪切删除时注册）：返回 `Result<(), ClipboardError>`。剪贴板服务只会在平台写入确认成功且选区上下文分毫未变之后调用它。请务必将所有可变编辑状态（如输入法组合状态，而不仅是文本长度或选中文本内容）纳入 `InspectSelection` 的上下文中，并在修改源数据前完成全部校验。
- **`PasteSelection`**（可选，支持粘贴导入时注册）：返回 `Result<(), ClipboardError>`。通过 `event.data()` 读取不可变的平台剪贴板载荷。内置 `TextEditing` 仅接受单个条目（item）并选取其 `text/plain` 表示（即使同一条目同时提供了 HTML 等格式），遇到多条目粘贴时会明确拒绝而非静默只取第一个。自定义组件可自行定义多条目与自定义 MIME 格式规则。

例如 Gallery 中的 Pixel Loom 在 `PixelSurface` 上注册了这四个请求：它的选区是当前正在编辑的像素帧（独立于动画预览），工具栏使用自定义 MIME 类型 `application/x-mirui-pixel-frame` 复制、剪切与粘贴包含尺寸和调色板的 12×12 索引像素帧，在落笔修改像素前先校验整份快照并记录一步撤销（Undo）。

## 数据表示与内存分配 (`ClipboardData`) {#data-and-allocation}

`ClipboardData` 包含若干 `ClipboardItem`，每个条目包含若干由 `ClipboardFormat` 描述的 MIME 表示。静态条目、格式、MIME 字符串与字节切片在整个生命周期内可直接保持借用；动态 `with_capacity` 构建器则预留显式的条目数、格式数与总字节上限，追加数据时既不会隐式扩容也不会静默截断。重复的 MIME 表示（忽略大小写）会被拒绝。`ClipboardData::text(text)` 创建一份独立的 UTF-8 文本快照。

显式发起的剪贴板操作允许分配快照、上下文值与平台格式转换缓冲，但绝不会给日常的逐帧绘制或模型/视图更新增加任何内存分配。在途的剪贴板请求复用首次剪贴板命令准备好的存储槽位，受 `App` 配置的命令记录数约束；平台读取结果也会在预留槽位内构造，若分配失败则返回 `ClipboardError::Capacity`。

## 各平台后端实现 {#platforms}

- **Web 浏览器**：使用异步 Clipboard API 并汇报浏览器的确认或拒绝结果。每次写入支持包含多种 MIME 表示的单个条目（多条目写入返回 `Unsupported` 而非静默丢弃）。纯文本、HTML 与 PNG 使用浏览器标准格式；其他 MIME 类型在浏览器支持时自动使用 Web 自定义格式（导入时自动剥离平台前缀）。此外，Web 原生 `copy` / `cut` / `paste` DOM 事件会直接驱动当前聚焦组件的同一套剪贴板能力，无需合成虚假按键事件；粘贴包含 `File` 对象时会先锁定目标选区，待所有文件异步读取完毕后一次性交付，若中途选区已变或任一文件读取失败则整体拒绝。
- **macOS（11 及以上）**：SDL 软件、SDL-GPU 与 WGPU 窗口共享原生 `NSPasteboard` 桥接，完整保留多条目与多 MIME 表示（纯文本、HTML、RTF、PNG、文件/URL 以及自定义二进制格式），并通过 `UniformTypeIdentifiers` 映射系统类型标识。若读取中途检测到系统剪贴板代数（changeCount）变化，会拒绝本次读取以避免混入不同时刻的快照。文件传输采用每个条目对应单个 `text/uri-list` URI 的规范。
- **Linux Wayland 与 X11**：Wayland 与 X11 桥接在后台管理选区所有权（selection ownership）、多格式 MIME 协商与分块非阻塞管道传输，带有总字节数、系统调用次数与绝对超时保护。剪切操作严格等待写入回执完成后才删除源文本；焦点丢失或窗口关闭会安全取消挂起的读取请求。
- **Android 与 iOS**：通过各自的宿主平台桥接与系统剪贴板通信，并在生命周期挂起/恢复期间维护会话一致性。
- **自定义 `Surface` 扩展**：实现 `write_clipboard(ClipboardWrite)` 与 `read_clipboard(ClipboardRead)`。同步完成可直接返回 `ClipboardDispatch::Complete(request.complete(result))`；异步完成则保留请求、返回 `Pending`，并在就绪后向输入队列推入 `InputEvent::Clipboard(request.complete(result))`。原生菜单栏触发的“全选/复制/剪切/粘贴”可通过 `InputEvent::ClipboardInput`（`copy()`、`cut()`、`select_all()`、`paste_requested()` 或携带完整快照的 `paste(data)`）送入当前聚焦组件。

## 原生契约测试 {#native-contract-checks}

Android 宿主库可作为 Gradle 依赖并入应用工程（测试需开启 `android.useAndroidX=true`）。在安装有 JDK 17、Android SDK 35 与 Gradle 8.11.1 的环境下可运行存储与 Lint 检查：

```bash
gradle -p platform/android testDebugUnitTest lintDebug assembleDebug
```

Wayland 与 X11 的协议、管道流、增量分块传输与工作线程回执契约测试均基于内存级协议夹具或私有 Unix 套接字运行，不会触碰宿主桌面的真实系统剪贴板。若本机安装了 `Xvfb`，还可显式运行隔离的 X11 原生服务契约测试：

```bash
cargo test --test x11_clipboard_source --no-default-features --features std native_contracts -- --ignored
cargo test --lib --no-default-features --features sdl native_worker_preserves_app_receipts_and_releases_server_resources -- --ignored
```

在 macOS 桌面会话中，可通过 `--menu-window`、`--menu-sdl` 或 `--menu-sdl-gpu` 运行 `native_clipboard` 集成测试，在隔离的具名剪贴板（named pasteboard）上验证 AppKit 菜单路由、响应链卸载与格式桥接，不会污染系统公共剪贴板。iOS 模拟器构建同样支持运行该测试目标。
