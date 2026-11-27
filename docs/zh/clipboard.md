# 跨平台剪贴板 {#clipboard}

`commands.clipboard().copy(editor)` 复制指定实体所属的选区。`TextInput` 以及挂载了 `TextEditing` 的自定义控件会自动提供该能力。反向的 UTF-8 选区（游标在锚点之前）会被自动规范化为正向范围；若选区为空，平台剪贴板保持原样不变。复制操作不会改变焦点、选区、文档内容或正在进行的输入法预编辑（composition）。

<MiruiSnippet recipe="clipboard-lab" label="Select All, Copy, Cut & Paste" file="recipes.rs" height="220px">

<template #exercise>

依次点击 `Select All` 与 `Copy`、`Cut` 或 `Paste`（每个按钮均挂载了 `PointerFocus::Preserve` 以保持编辑器焦点），观察下方 `ClipboardFinished` 完成状态的实时更新。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="从命令触发、选区检查与导出、宿主平台桥接到提交编辑与完成通知的剪贴板四阶段生命周期">
  <svg viewBox="0 0 880 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="clip-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="194" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · COMMAND / SHORTCUT</text>
    <text x="32" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">显式目标与顺序队列</text>
    <text x="32" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">clipboard().copy / cut / paste</text>
    <text x="32" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Ctrl/Cmd + A / C / X / V</text>
    <text x="32" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">锁定实体与列表行身份</text>
    <path d="M210 84H248" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#clip-arrow)" />
    <rect x="256" y="20" width="194" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="272" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · INSPECT &amp; EXPORT</text>
    <text x="272" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">上下文与不可变快照</text>
    <text x="272" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">InspectSelection</text>
    <text x="272" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">CopySelection</text>
    <text x="272" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">修订号守卫与零修改导出</text>
    <path d="M450 84H488" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#clip-arrow)" />
    <rect x="496" y="20" width="186" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="512" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · HOST BRIDGE</text>
    <text x="512" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">同步或异步宿主桥接</text>
    <text x="512" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Surface::write_clipboard</text>
    <text x="512" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Surface::read_clipboard</text>
    <text x="512" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">多 MIME 表示与格式校验</text>
    <path d="M682 84H720" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#clip-arrow)" />
    <rect x="728" y="20" width="136" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="744" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · COMMIT &amp; NOTIFY</text>
    <text x="744" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">确认落盘与回执</text>
    <text x="744" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">CutSelection / PasteSelection</text>
    <text x="744" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ClipboardFinished</text>
    <text x="744" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">回执确认后原子修改</text>
  </svg>
</div>

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

请在**目标编辑器**（而非工具栏上的“复制/粘贴”按钮）上监听 `mirui::ui::clipboard::ClipboardFinished`。其中的操作 ID 唯一标识一次请求，`result` 字段区分各完成状态与错误：

| `ClipboardFinished.result` | 平台剪贴板状态 | 编辑器 / 组件状态 | 典型触发场景 |
| --- | --- | --- | --- |
| `Ok(ClipboardOutcome::Copied)` | 已更新为导出的不可变快照 | 保持不变（选区、焦点与预编辑原样保留） | 非空选区 `copy` 经平台确认写入成功 |
| `Ok(ClipboardOutcome::Cut)` | 已更新为导出的不可变快照 | 收到写入回执后删除选区；光标折叠至起点并退役旧 IME 会话 | `cut` 平台写入成功，且等待期间选区上下文未发生任何变化 |
| `Ok(ClipboardOutcome::CopiedWithoutCut)` | 已更新为导出的不可变快照 | 保持不变（安全跳过删除操作） | `cut` 平台写入成功，但等待期间源实体/选区/预编辑已变或 `CutSelection` 拒绝删除 |
| `Ok(ClipboardOutcome::Pasted)` | 仅读取，不修改系统剪贴板 | 替换选区（或在光标处插入）；光标移至新文本末尾并退役旧 IME 会话 | `paste` 平台读取成功、上下文校验一致且 `PasteSelection` 接受载荷 |
| `Ok(ClipboardOutcome::EmptySelection)` | 保持不变 | 保持不变 | 调用 `copy` 或 `cut` 时 `InspectSelection` 汇报 `has_selection == false` |
| `Err(ClipboardError)` | 写入被拒或读取失败 | 保持不变（文档、选区与预编辑原样保留） | 权限被拒、`Unsupported` 格式或多条目、`Capacity` 容量超限、上下文过期或宿主 `Unavailable` |

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

| 平台 / 宿主桥接 | 执行模型 | 条目与 MIME 格式支持 | 文件 / 图像转换与菜单集成 |
| --- | --- | --- | --- |
| **Web** (`navigator.clipboard` + DOM 事件) | 异步读写；纯文本粘贴同步，`File` 粘贴异步 | 写入支持单条目多 MIME；读取与 `File` 粘贴支持多条目 | 标准 `text/plain`、`text/html`、`image/png` + Web 自定义格式；原生 DOM `copy`/`cut`/`paste` |
| **macOS 11+** (`NSPasteboard` + `UniformTypeIdentifiers`) | 主线程同步完成 | 多条目、多 MIME（纯文本、HTML、RTF、PNG、URI、二进制） | 每个条目单个 `text/uri-list`；TIFF → PNG；AppKit 响应链原生菜单路由 |
| **Windows** (Win32 + WIC) | 绑定窗口句柄同步完成 | 单条目多 MIME（文本、HTML、RTF、PNG、URI 列表、注册 MIME） | 通过 WIC 双向转换 PNG ↔ BGRA 位图；直接提供原生文件路径而不打开文件 |
| **Linux X11** (`x11rb` + `XFixes` / 时间戳) | 独立显示连接上的异步工作线程 | 单条目多 MIME（含二进制载荷与增量分块传输） | 选区所有权（selection ownership）跟踪；超大载荷 `INCR` 增量流式传输 |
| **Linux Wayland** (`wl_data_device`) | 宿主事件循环驱动的非阻塞管道 | 单条目多 MIME（含二进制载荷） | 要求窗口持有键盘焦点与真实输入序列号；服务端同步回执 |
| **iOS 14+** (`UIPasteboard`) | 主线程同步提交 | 多条目、多 MIME（文本、HTML、RTF、PNG、URI、二进制） | `UIImage` 自动应用朝向并转为 PNG；每个条目单个 URI |
| **Android** (`ClipData` + `platform/android` 提供方) | 异步 Java 工作线程 + `CancellationSignal` | 多条目、多 MIME（通过私有快照文件 URI 流式传输） | 跨进程重建保留快照；应用挂起时安全取消在途任务 |
| **回退 / 自定义 `Surface`** | 同步 `Complete` 或异步 `Pending` | SDL 回退仅支持单条目 `text/plain`；自定义由 `Surface` 决定 | `write_clipboard` / `read_clipboard` + `InputEvent::ClipboardInput` |

### Web Clipboard API {#web-clipboard-api}

Web 端使用异步 Clipboard API 并汇报浏览器的确认或拒绝结果。每次写入支持包含多种 MIME 表示的单个条目（多条目写入返回 `Unsupported` 而非静默丢弃）。纯文本、HTML 与 PNG 使用浏览器标准格式；其他 MIME 类型在浏览器支持时自动使用 Web 自定义格式（导入时自动剥离平台前缀）。读取操作会保留浏览器返回的全部条目与表示供目标组件检查。浏览器剪贴板访问仍受用户激活状态（user activation）、格式支持与平台权限约束。

此外，Web 原生 `copy`、`cut` 与 `paste` DOM 事件会直接驱动当前聚焦组件的同一套剪贴板能力，无需合成虚假按键事件。复制与剪切要求异步写入得到确认；原生粘贴在事件过期前接管事件中的字符串表示与 `File` 对象，不会再次读取系统剪贴板或插入隐藏 `<textarea>` 内容。纯字符串原生粘贴保持同步完成；包含 `File` 对象时则会先在输入投递瞬间锁定目标选区，待所有文件按序异步读取完毕后一次性交付（若中途选区已变或任一文件读取失败则整体拒绝）。DOM 事件中的字符串表示归为一个条目，每个 `File` 各自构成一个包含文件字节与 MIME 类型（缺失时回退为 `application/octet-stream`）的独立条目。

### macOS NSPasteboard {#macos-nspasteboard}

在 macOS 11 及以上版本中，SDL 软件、SDL-GPU 与 WGPU 窗口共享原生 `NSPasteboard` 桥接，完整保留多条目与多 MIME 表示（包括纯文本、HTML、RTF、PNG、文件/URL 条目以及自定义二进制格式），并通过 `UniformTypeIdentifiers` 转换系统类型标识。导入 TIFF 时在保留原始表示的同时额外提供 PNG。若读取中途检测到系统剪贴板代数（`changeCount`）变化，会拒绝本次读取以避免混入不同时刻的快照。

macOS 原生文件传输采用“每个条目单个 URI”的规范：多个文件对应多个 `ClipboardItem`，每个条目携带各自的 `text/uri-list` 表示；单个条目内包含多个 URI 会在写入前直接返回 `Unsupported`。非法 UTF-8、非法 PNG、相对 URI 以及冲突的原生类型映射会在清空系统剪贴板之前被提前拒绝。所有 `NSPasteboard` 操作均在窗口所属的主线程上运行并返回同步完成结果（跨线程调用返回 `Unavailable`）。macOS 下的 SDL 软件、SDL-GPU 与 WGPU 窗口通过 AppKit 响应链（responder chain）与宿主事件循环直接路由原生菜单的 Copy、Cut、Paste 与 Select All 动作。

### Windows、X11 与 Wayland {#windows-x11-and-wayland}

在 Windows 上，SDL 软件、SDL-GPU 与 WGPU 共享原生 Win32 剪贴板桥接。单个条目可包含多种表示：Unicode 文本、HTML 片段、RTF、PNG、文件 URI 列表以及自定义注册的 MIME 数据（多条目写入返回 `Unsupported`）。导出 PNG 时会同时提供原生 BGRA 位图；导入原生位图时则通过 Windows Imaging Component (WIC) 转换为 PNG。文件 URI 列表会同时提供原生文件路径而不打开文件本身。写入前会使用表面自带的窗口句柄预先准备好所有格式与内存块；读取则在单次打开剪贴板期间拷贝完整快照并立即关闭句柄，随后再执行图像解码。被其他进程占用的剪贴板直接返回 `Unavailable`，不进行阻塞重试。

在 Linux X11 上，SDL 软件、SDL-GPU 与 WGPU 共享异步选区（selection）桥接。单个条目支持多种 MIME 表示与二进制数据，超大表示自动采用 `INCR` 增量分块传输。支持 `XFixes` 时，读取操作通过最终服务端事件跟踪选区所有权声明；无 `XFixes` 时则校验源时间戳。X11 桥接按需延迟打开独立的显示连接，仅使用一个请求槽位与一个完成槽位，关闭 `Surface` 时可立即取消套接字等待而无需在 UI 线程阻塞等待工作线程退出。

在 Linux Wayland 上，SDL 软件、SDL-GPU 与 WGPU 复用窗口原生的 display 与 seat。操作要求窗口拥有键盘焦点与真实输入序列号（serial）。读取操作绑定到单次选区提供项（selection offer），若中途焦点或选区改变则立即失败；源端传输保留已捕获的数据快照，通过宿主事件循环推进有界非阻塞管道读写（带有总字节数、系统调用次数与绝对超时保护），并在服务端同步（server sync）后确认写入完成。

### 移动端与嵌入式目标 {#mobile-and-embedded-targets}

在 iOS 14 及以上版本中，移动端 WGPU、软件上传与 SDL 表面共享同一套 UIKit `UIPasteboard` 主线程桥接。写入前一次性准备好全部条目与表示；读取时一次性捕获所有条目（保留 UTF-8 文本、HTML、RTF、PNG、单条目单 URI 以及自定义二进制数据，原生 `UIImage` 自动应用显示朝向并转为 PNG）。

Android 端的 WGPU、软件上传、SDL 与 SDL-GPU 共享异步桥接。宿主任务运行在按需启动的 Java 工作线程上，应用挂起（suspension）时通过 `CancellationSignal` 安全取消在途任务而不会阻塞 UI 线程或泄漏 `Activity`。`platform/android` 宿主库提供了一个非导出的、支持 URI 临时授权的剪贴板 `ContentProvider`，通过私有快照文件跨进程重建保留每个条目的多种 MIME 表示。

在其他桌面或嵌入式平台上，SDL 软件与 GPU 表面通过 SDL 剪贴板读写单个 `text/plain` 纯文本表示（显式拒绝其他多格式或多条目而非静默丢弃）。自定义 `Surface` 可实现 `write_clipboard(ClipboardWrite)` 与 `read_clipboard(ClipboardRead)`：同步完成直接返回 `ClipboardDispatch::Complete(request.complete(result))`；异步完成保留请求、返回 `Pending`，并在就绪后向输入队列推入 `InputEvent::Clipboard(request.complete(result))`。原生菜单动作可通过 `InputEvent::ClipboardInput`（`copy()`、`cut()`、`select_all()`、`paste_requested()` 或携带完整快照的 `paste(data)`）送入当前聚焦组件。

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
