# 滚动容器与惯性控制 {#scrolling}

软件帧缓冲传输（scroll transfer）利用当前滚动偏移与上一次成功绘制偏移之间的差值直接平移已有像素。嵌套滚动传输会复用已绘制的源窗口；若裁剪区域变化或几何形状不兼容则自动转为重绘。输入处理器本身不单独维护另一套渲染差值。

<MiruiSnippet recipe="scroll" label="ScrollOffset" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-scroll{rust}

<template #exercise>

点击 `Top (#1)`、`Middle (#4)` 或 `Bottom (#7)` 通过弹簧过渡驱动 `scroll_offset`，或直接在列表中拖拽 / 滚动滚轮。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="滚动视口裁剪、帧缓冲平移与虚拟化 LazyList 单元格池">
  <svg viewBox="0 0 880 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="scr-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="256" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · INPUT &amp; MOTION</text>
    <text x="32" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">拖拽、滚轮与程序化滚动</text>
    <text x="32" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ScrollOffset · ScrollConfig</text>
    <text x="32" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">commands.scroll_into(target)</text>
    <text x="32" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">惯性滑行、越界橡皮筋与弹簧</text>
    <path d="M272 84H314" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#scr-arrow)" />
    <rect x="322" y="20" width="256" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="338" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · VIEWPORT &amp; CELLS</text>
    <text x="338" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">裁剪视口与虚拟单元格池</text>
    <text x="338" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">clip_children: true · LazyList</text>
    <text x="338" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">visible_start + 两侧各 2 个备用行</text>
    <text x="338" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">纯位移更新直接跳过重排</text>
    <path d="M578 84H620" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#scr-arrow)" />
    <rect x="628" y="20" width="236" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="644" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · DAMAGE PLAN</text>
    <text x="644" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">像素拷贝 + 新露出条带重绘</text>
    <text x="644" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">app.dirty_regions()?</text>
    <text x="644" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Scroll transfer + repaint</text>
    <text x="644" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">软件帧缓冲直接复用未变像素</text>
  </svg>
</div>

## 查询待绘制的脏区域 {#inspecting-pending-drawing}

`app.dirty_regions()?` 以逻辑像素为单位借用待处理的重绘矩形与滚动传输区域。`affected_regions()` 同时涵盖这两类覆盖范围；`app.dirty_region()?` 则返回它们的总外接矩形。查询操作会准备响应式状态与布局，但不会推进动画时间或消费待绘制状态；在查询之后发生的修改会在下一次调用 `render_dirty()` 生成绘制计划时一并纳入。

```rust
use mirui::prelude::*;

let mut app = App::headless(240, 120);
app.with_default_widgets();
app.spawn_root().id();
let coverage = app.dirty_region()?;
let pending = app.dirty_regions()?;
assert_eq!(coverage, pending.bounding_rect());
app.render_dirty()?;
# Ok::<(), mirui::render::RenderError>(())
```

该查询返回的是一份只读快照，而非冻结的底层渲染命令：渲染钩子、后续状态修改以及具体后端的能力都可能改变最终刷屏计划。只有当一帧绘制成功完成时才会确认清除已准备的脏区；若准备或绘制中途失败，待处理脏区会原样保留。首帧、窗口尺寸变更以及临时后备缓冲（transient backbuffer）会自动升级为全屏重绘。

重绘矩形会被规范化为互不重叠的区域；当预备的矩形槽位充足时会保留稀疏脏区之间的空隙，碎片过多时则保守合并。透明的布局容器不会把整个自身矩形塞进脏区，而是分别测量各自的子节点。对于无法使用帧缓冲像素拷贝的滚动（例如带圆角裁剪的容器内滚动），子节点的旧位置与新位置会一并计入重绘范围。被隐藏或删除的子树在移出布局树前也会把最后一次绘制的覆盖区计入擦除范围。

## 输入物理与视口坐标 {#input-and-viewport}

滚动默认是连续的，不会强制吸附到行边界。手指拖拽与滚轮共享边界橡皮筋阻力（elastic resistance）与松手后的运动状态；持续拖拽时内容紧随手指而不产生并发回弹，松手后则根据近期移动速度进入惯性滑行（若松手前手指已停稳则不触发滑行）。在合法滚动范围内，触控板与滚轮直接应用亚像素级增量，完整保留宿主操作系统提供的动量曲线。单纯改变滚动偏移只会使绘制与命中几何失效；只有当虚拟单元格重新绑定或尺寸发生变化时才会触发重新布局。帧缓冲像素平移复用仅限于未旋转缩放、无边框且无圆角的矩形视口上的整物理像素位移，其余情况自动重绘受影响的视口。

`InputEvent::Wheel` 与强类型 `Wheel` 事件载荷中的 `dx`、`dy`、`x` 与 `y` 均使用逻辑像素：正 `dx` 增加水平滚动偏移，正 `dy` 减小垂直滚动偏移。各平台后端在转换为内部定点数前已完成像素归一化：
- **Web 浏览器**：像素模式增量按画布当前的 CSS 与逻辑像素比缩放；行模式（line delta）每行折算 20 逻辑像素；页模式（page delta）按视口逻辑宽高折算；
- **WGPU**：像素增量除以窗口缩放因子（scale factor）；
- **SDL / Linux / NuttX**：精确滚轮值与整数刻度统一按每格 20 逻辑像素折算。`Rotary.delta` 则保留物理旋钮编码器的原始步数。

关于桌面端 SDL 运行环境与 macOS 触控板配置，请参阅[环境搭建与多端工程](quickstart.md#desktop-sdl)。

## 程序化滚动与边界观察 {#observing-content-movement}

- **`commands.scroll_into(row)`**：以最小滚动幅度驱动各层可滚动祖先，使目标控件完整露出；若目标已在可视区内则保持当前位置与动画不变；
- **`commands.scroll_to(container, row)`**：将目标后代控件对齐到滚动容器的起始边缘；
- **`commands.scroll_to(container, ScrollOffset { x, y })`**：直接滚动到指定的内容偏移坐标（自动钳制在合法内容边界内并遵循容器启用的滚动轴）。

所有滚动命令均在布局计算完成后执行，因此同一回调内刚刚修改的尺寸或新增的行也能被准确计入：

```rust,ignore
#[event]
fn show_selected(row: Entity, commands: commands!()) {
    commands.scroll_into(row);
}
```

对于定高/定宽的 `LazyList`，甚至无需目标行当前已被实例化为实体，直接传入**数据下标（`u32`）**或**稳定条目身份（`ModelItemId`）**即可滚动到位：

```rust,ignore
commands.scroll_into((list, 50_000_u32));
commands.scroll_into((list, selected_id));
commands.scroll_to(list, selected_id);
commands.scroll_to(list, 50_000_u32);
```

### 程序化滚动过渡 (`ScrollConfig::with_transition`) {#programmatic-motion}

在容器的 `ScrollConfig` 上配置弹簧或补间过渡：

```rust
use mirui::anim::{SNAPPY, TweenConfig, ease};
use mirui::input::event::scroll::ScrollConfig;

let spring = ScrollConfig::default().with_transition(SNAPPY);
let tween = ScrollConfig::default().with_transition(TweenConfig::new(160, &ease::OUT_QUAD));
```

默认 `transition: None` 会立即跳往目标位置。配置 `with_transition` 后，`scroll_into` 与 `scroll_to` 会沿弹簧或补间曲线平滑过渡（中途再次发起滚动会平滑重定向），而手指拖拽与滚轮依然保持各自的物理惯性。

### 监听滚动通知 (`ScrollStarted` / `ScrollChanged` / `ScrollEnded`) {#scroll-notifications}

对于直接修改滚动偏移的命令式操作，可使用 `commands.set::<mirui::ui::property::prop::ScrollOffset>(node, ScrollOffset { x, y })`；在组合函数中则直接绑定 `scroll_offset: ${position}`（支持通过 `prop!(ScrollOffset)` 转发）。

使用 `on ScrollChanged` 观察实际偏移变化，使用 `on ScrollStarted` 与 `on ScrollEnded` 观察完整的滚动片段（包含程序化写入、亚像素精度与越界橡皮筋位置；`ScrollEnded` 在惯性滑行与回弹彻底静止后才触发，而非手指抬起瞬间）。三者均支持在挂载了 `ScrollOffset` 的节点上使用内联代码块、闭包或 `#[event]` 回调；详见[控件初始化、输入与通知](widget-events.md#scroll-notifications)。

若需静态读取上一次成功布局算出的视口大小、内容总长与最大可滚动偏移，可查询 `query!(ScrollBounds)`。

## 虚拟化单元格 (`LazyList`) {#virtualized-cells}

`LazyList` 在首次非空布局后按需创建单元格实体，并在滚动过程中循环复用它们。只需提供 `items`、单个单元格的尺寸 `item_extent` 以及一个返回新单元格根实体的 `on CreateCell` 回调即可。`items` 既接受纯数量（`u32`，无需创建模型），也接受已注册的 `ModelList` 或 `ModelMap` 实例。

列表会自动维护可视区起始下标 `visible_start` 以及两侧各最多 **2 个备用行槽位（spare slots）**。当视口滚动时，仍处于保留范围内的行保持原有实体不变；滚出范围的空闲单元格被隐藏并复用于新进入视口的下标。

`CreateCell` 仅在池中需要新建物理单元格实体时调用一次。其 `event.item` 是一个由框架管理的 `ListItem` 模型，当单元格被复用到新行时，其可观察的 `index()` 与 `id()` 会自动更新。请始终在响应式属性 `${...}` 与事件回调内部读取 `item.index()` 或 `item.id()`，切勿在创建单元格时把下标当成普通整数捕获死：

```rust,ignore
use mirui::ui::widgets::lazy_list::CreateCell;

#[event]
fn create_row(event: event!(CreateCell)) -> Entity {
    const LABELS: [&str; 4] = ["ALPHA", "BETA", "GAMMA", "DELTA"];
    let item = event.item.clone();
    ui! {
        Text(text: ${ LABELS[item.index() as usize % LABELS.len()] })
    }
}

#[compose]
fn rows_panel() {
    ui! {
        LazyList (height: 240, items: 100_u32, item_extent: Fixed::from_int(24))
        on CreateCell(create_row::callback())
    };
}
```

当单元格重新绑定到另一条数据时，其上未完成的指针捕获、焦点与待决输入会自动取消（不会打断外层列表正在进行的滚动拖拽）。预热完成后的日常滚动（包括隐藏与复用单元格）全程**零框架堆内存分配**。
