<script setup>
import { computed, ref, watch } from 'vue';
import { useData } from 'vitepress';

const props = defineProps({
  kind: { type: String, required: true },
});

const { lang } = useData();
const zh = computed(() => lang.value === 'zh-CN');
const activeScenario = ref(0);
const activeStep = ref(0);

watch(activeScenario, () => {
  activeStep.value = 0;
});

const SPECS = {
  'reactive-dataflow': {
    badge: 'REACTIVE FLUSH PIPELINE',
    titleEn: 'Single-frame dirty coalescing & partial damage',
    titleZh: '响应式单帧合并刷新与局部脏区管线',
    mode: 'pipeline',
    nodes: [
      { id: 'mutate', tag: '01 · MUTATE', en: 'Model / Signal Write', zh: 'Model / Signal 写入', sub: 'gauge.step(10)' },
      { id: 'queue', tag: '02 · BITMAP', en: 'Dirty Subscriber Queue', zh: '位图去重入队', sub: 'source × consumer u64' },
      { id: 'flush', tag: '03 · FLUSH', en: 'flush_signal_dirty', zh: '单帧合并刷出', sub: 'Property::apply' },
      { id: 'damage', tag: '04 · DAMAGE', en: 'Layout & Partial Paint', zh: '布局与局部重绘', sub: 'PrevRect ∪ CurrRect' },
    ],
    scenarios: [
      {
        en: 'Single write',
        zh: '单次修改字段',
        steps: [
          {
            node: 0,
            en: 'A callback mutates `gauge.level` from 40 to 50. At the outermost method return, the generated handle compares the `#[observe]` snapshot (`40 != 50`).',
            zh: '回调将 `gauge.level` 从 40 改为 50。在最外层可变方法返回时，生成的句柄比对 `#[observe]` 前后快照（`40 != 50`）。',
            code: 'gauge.step(10); // 40 -> 50 (snapshot changed)',
            metric: 'Allocations: 0 B · Widget tree touched: No',
          },
          {
            node: 1,
            en: 'The dependency bitmap marks the two widget property bindings (`Text.text` and `View.width`) dirty in the pending queue without running them mid-callback.',
            zh: '依赖位图将订阅了 `level()` 的两个属性绑定（`Text.text` 与 `View.width`）标记为脏并推入队列，不会在回调中途直接改写控件树。',
            code: 'dirty_bitmap.mark(source_id, [prop_text, prop_width]);',
            metric: 'Queued bindings: 2 · Mid-callback reflows: 0',
          },
          {
            node: 2,
            en: 'On the frame schedule (after systems, before layout), `flush_signal_dirty` drains the queue once and applies the new text and percentage width.',
            zh: '进入帧调度阶段（系统执行完毕后、布局计算前），`flush_signal_dirty` 一次性排空队列，应用新的文本与百分比宽度。',
            code: 'flush_signal_dirty(&mut world); // applies 2 properties',
            metric: 'Evaluated getters: 2 · Rebuilt entities: 0',
          },
          {
            node: 3,
            en: 'Layout solves the updated bar width and records `PrevRect ∪ CurrRect` into `dirty_regions()`. The renderer repaints only those two rectangles.',
            zh: '布局阶段算出新的进度条宽度，并将 `PrevRect ∪ CurrRect` 记入 `dirty_regions()`；渲染器仅重绘这两小块物理像素。',
            code: 'surface.flush(PhysicalRect { x: 18, y: 62, w: 140, h: 14 });',
            metric: 'Repainted area: ~8% of screen',
          },
        ],
      },
      {
        en: 'Coalesced 3× burst',
        zh: '同帧连续 3 次修改合并',
        steps: [
          {
            node: 0,
            en: 'Three input events arrive in the same frame batch (`+10`, `+10`, `+10`), stepping `level` from 40 → 50 → 60 → 70 before the next frame render.',
            zh: '同一帧输入批次内连续触发三次修改（`+10`、`+10`、`+10`），`level` 依次从 40 → 50 → 60 → 70。',
            code: 'gauge.step(10); gauge.step(10); gauge.step(10); // final = 70',
            metric: 'Model writes: 3 · Intermediate renders: 0',
          },
          {
            node: 1,
            en: 'Because the dirty queue deduplicates subscribers via a bitset, `prop_text` and `prop_width` are enqueued only once for the entire burst.',
            zh: '由于待处理脏队列通过位图去重，`prop_text` 与 `prop_width` 在这三次写入中仅入队一次。',
            code: 'if !queued_bits.test_and_set(consumer_id) { queue.push(consumer_id); }',
            metric: 'Duplicate queue entries: 0',
          },
          {
            node: 2,
            en: '`flush_signal_dirty` runs each binding closure once, reading the final value `70` directly—skipping `50` and `60` completely.',
            zh: '`flush_signal_dirty` 对每个绑定只执行一次闭包，直接读取最终值 `70`，完全跳过中间的 `50` 与 `60`。',
            code: 'Property::apply(world, entity, /* reads 70 once */);',
            metric: 'String formatting passes: 1 (for 70%)',
          },
          {
            node: 3,
            en: 'Layout and rasterization run once for the transition from `40%` to `70%`.',
            zh: '布局与光栅化仅针对 `40% → 70%` 执行一次。',
            code: 'app.render_dirty()?; // single coalesced frame',
            metric: 'Frame passes: 1',
          },
        ],
      },
      {
        en: 'Reverted write (No-op)',
        zh: '同事务内改完还原（零通知）',
        steps: [
          {
            node: 0,
            en: 'A mutating method modifies `self.done = true` and then restores `self.done = false` before returning (or returns `Err(...)`).',
            zh: '可变方法在内部先修改字段又改回原值（或中途校验失败返回 `Err(...)`）。',
            code: 'self.done = !self.done; self.done = !self.done; // net change = none',
            metric: 'Snapshot comparison: before == after',
          },
          {
            node: 1,
            en: 'Because the post-transaction snapshot equals the pre-transaction snapshot, zero subscribers are marked dirty.',
            zh: '由于事务结束时的字段快照与事务开始前完全相等，没有任何订阅者被标脏。',
            code: '// No subscribers enqueued; dirty queue stays empty',
            metric: 'Queued bindings: 0',
          },
          {
            node: 2,
            en: '`flush_signal_dirty` sees an empty queue and returns immediately.',
            zh: '`flush_signal_dirty` 检测到队列为空，立即返回。',
            code: 'if dirty_queue.is_empty() { return; }',
            metric: 'Flush cost: 0 ns',
          },
          {
            node: 3,
            en: 'Zero layout passes and zero `Surface::flush` calls occur—an idle frame lets the MCU or host thread sleep.',
            zh: '不触发重新布局，也不调用 `Surface::flush`——静止帧允许 MCU 直接进入 `wfi` 休眠。',
            code: 'assert!(app.dirty_regions()?.is_empty());',
            metric: 'SPI / GPU bus traffic: 0 B',
          },
        ],
      },
    ],
  },
  'event-routing': {
    badge: 'TYPED EVENT DISPATCH',
    titleEn: 'Gesture bubbling, double-buffered notifications & deferred commands',
    titleZh: '手势冒泡、双缓冲通知队列与延迟命令时序',
    mode: 'pipeline',
    nodes: [
      { id: 'input', tag: '01 · RECOGNIZE', en: 'Hit Test & Gesture', zh: '几何刷新与手势识别', sub: 'Tap(n) · Drag · Key' },
      { id: 'route', tag: '02 · SNAPSHOT', en: 'Frozen Route Selection', zh: '冻结路由与代数校验', sub: 'Parent chain / Focus' },
      { id: 'exec', tag: '03 · EXECUTE', en: '#[event] Handler', zh: '受控作用域执行', sub: 'event! · query! · res!' },
      { id: 'commit', tag: '04 · COMMIT', en: 'Commands & FIFO Drain', zh: '延迟命令与乒乓排空', sub: 'commands!() -> Buffer B' },
    ],
    scenarios: [
      {
        en: 'EntityBubble (Tap / Drag)',
        zh: 'EntityBubble（手势冒泡与拦截）',
        steps: [
          {
            node: 0,
            en: 'Before hit testing a spatial pointer event, `App` refreshes any stale layout or transform geometry so coordinates never hit last frame’s stale boxes.',
            zh: '在对空间指针事件做命中测试前，`App` 先刷新过期的布局与变换几何，确保坐标永远命中最新位置。',
            code: 'InputEvent::PointerUp { x, y, .. } -> Gesture::Tap { count: 2 }',
            metric: 'Route: EntityBubble (target -> ancestors -> root)',
          },
          {
            node: 1,
            en: 'The listener pool freezes the target’s `Parent` chain and checks entity generations. Reparenting or deleting nodes inside a callback cannot corrupt the active walk.',
            zh: '监听器池先行冻结从命中目标到根节点的 `Parent` 链快照，并在每一步执行前核对实体代数（generation）。',
            code: 'snapshot = freeze_bubble_chain(target_entity);',
            metric: 'Eligibility: skips Hidden / Disabled subtrees',
          },
          {
            node: 2,
            en: 'Handlers on each node run in installation order: internal View hooks first, then authored `on Tap` / `on Tap(2)`. Returning `()` or `true` stops propagation; `false` continues.',
            zh: '节点上的处理器按安装顺序运行：先执行内部 `View` 钩子，再执行用户书写的 `on Tap` / `on Tap(2)`。返回 `false` 允许继续冒泡，返回 `()` 或 `true` 立即停止。',
            code: 'View() on Tap { model.record_click(); false } on Tap(2) { model.open(); }',
            metric: 'BubbleControl::Allow vs BubbleControl::Stop',
          },
          {
            node: 3,
            en: 'Once all parameter borrows (`query!`, `res!`) end, queued `commands!()` commit in source order before the next input event is processed.',
            zh: '待回调函数返回、所有 `query!` 与 `res!` 参数借用释放后，记录在 `commands!()` 中的结构修改按源码顺序立即提交。',
            code: 'commands.insert(entity, Selected); // committed before next input',
            metric: 'Default quota: 5 commands / callback',
          },
        ],
      },
      {
        en: 'EntityNotification (Ping-Pong FIFO)',
        zh: 'EntityNotification（控件通知双缓冲）',
        steps: [
          {
            node: 0,
            en: 'A widget input handler (such as `Slider` drag or `TextEditing` Enter key) calls `commands.emit(entity, Slider::ValueChanged { new, old })`.',
            zh: '内置或自定义控件在输入回调中调用 `commands.emit(entity, Slider::ValueChanged { new, old })`。',
            code: 'commands.emit(node, ValueChanged { new: Fixed::from_int(72), old });',
            metric: 'Compile-time check: WidgetSource<Slider>',
          },
          {
            node: 1,
            en: 'If the entity has no listener for `ValueChanged`, `emit` drops the payload immediately with zero arena usage. Otherwise it moves into Notification Buffer A.',
            zh: '若该实体未安装对应类型的监听器，`emit` 直接丢弃载荷（零缓冲区消耗）；否则载荷移动进入通知缓冲区 A。',
            code: 'if pool.has_listener::<ValueChanged>(node) { arena_a.push(payload); }',
            metric: 'Default preset: 8 notification records / buffer',
          },
          {
            node: 2,
            en: 'During the drain pass, listeners borrow stable read-only payloads from Buffer A (`&ValueChanged`). Any new notifications emitted inside that listener write into Buffer B.',
            zh: '排空阶段，监听器从缓冲区 A 稳定地只读借用载荷（`&ValueChanged`）；回调内部若再次发出新通知，则写入独立的缓冲区 B。',
            code: 'Slider(min: 0, max: 100) on ValueChanged { model.set_vol(event.new); }',
            metric: 'Zero cloning · No recursive borrow panic',
          },
          {
            node: 3,
            en: 'Buffer A finishes completely before Buffer B is swapped in, guaranteeing strict FIFO delivery up to the outer flush limit (16 deliveries).',
            zh: '缓冲区 A 当前批次全部执行完毕后才切换排空缓冲区 B，在不发生递归重入的前提下保证严格的 FIFO 顺序。',
            code: 'swap(arena_a, arena_b); // bounded by flush_limit (16)',
            metric: 'FIFO order preserved across chains',
          },
        ],
      },
      {
        en: 'InputFallback (Focus → App)',
        zh: 'InputFallback（键盘/滚轮回退路由）',
        steps: [
          {
            node: 0,
            en: 'A hardware key (`Key`), committed text (`TextCommitted`), `Wheel`, or `Rotary` event arrives from the platform `Surface`.',
            zh: '平台 `Surface` 投递按键（`Key`）、已提交文本（`TextCommitted`）、滚轮（`Wheel`）或表冠旋钮（`Rotary`）事件。',
            code: 'InputEvent::Key(Key { code: KEY_RETURN, pressed: true, .. })',
            metric: 'Route: Focused Entity -> Ancestors -> App',
          },
          {
            node: 1,
            en: 'For `Key` and `TextCommitted`, the route freezes the currently focused node and its ancestors. For `Wheel` and `Rotary`, the internal scroll route runs first.',
            zh: '对于按键与文本，路由先冻结当前聚焦节点及其祖先链；对于滚轮与旋钮，内置滚动处理器优先尝试消费。',
            code: 'focused_chain = freeze_focus_route(focused_entity);',
            metric: 'Consumed editor keys never leak to App shortcuts',
          },
          {
            node: 2,
            en: 'If the focused `TextInput` or node handler consumes the key, propagation stops. Only unconsumed input reaches `app.on_event(Key::EVENT, ...)`.',
            zh: '若聚焦的 `TextInput` 或节点处理器消费了该按键，路由即刻终止；只有未被消费的按键才会落入 `app.on_event(Key::EVENT, ...)`。',
            code: 'app.on_event(Key::EVENT, global_shortcuts::callback(model));',
            metric: 'Prevents WASD shortcuts firing while typing in search',
          },
          {
            node: 3,
            en: 'If `Ctrl/Cmd + A/C/X/V` was not consumed by any `Key` handler, the focused widget’s selection/clipboard provider runs as the final default action.',
            zh: '若 `Ctrl/Cmd + A/C/X/V` 未被任何按键回调消费，最后会自动触发当前聚焦组件的默认全选或剪贴板动作。',
            code: 'default_selection_or_clipboard_action(focused_snapshot);',
            metric: 'Target locked before callbacks run',
          },
        ],
      },
    ],
  },
  'thread-sync': {
    badge: 'MULTI-THREAD OWNERSHIP & SYNC',
    titleEn: 'Cross-thread texture handoff & ThreadOwner lifecycle',
    titleZh: '多线程纹理移交、像素守卫与 ThreadOwner 回收时序',
    mode: 'swimlane',
    lanes: [
      { en: 'Main App Thread (UI / World)', zh: 'App 主线程 (UI / World)' },
      { en: 'Background Worker Thread', zh: '后台 Worker 线程' },
      { en: 'GPU / SDL ThreadOwner', zh: 'GPU / SDL 线程所有者' },
    ],
    scenarios: [
      {
        en: 'Worker decode → Main UI',
        zh: '后台线程解码 → 移交主线程显示',
        steps: [
          {
            from: 0,
            to: 1,
            label: 'spawn(compressed_bytes)',
            en: 'Main thread spawns a background worker with raw file bytes. `App`, `World`, and `Model` stay strictly on the Main Thread.',
            zh: '主线程将压缩字节交给后台 Worker 线程。`App`、`World` 与 `Model` 严格留在主线程，无需加锁。',
            code: 'std::thread::spawn(move || Texture::from_vec(rgba, w, h, RGBA8888));',
            metric: 'Main thread frame blocking: 0 ms',
          },
          {
            from: 1,
            to: 1,
            label: 'Texture::from_vec(rgba)',
            en: 'Worker decodes pixels into a `Vec<u8>` and wraps it in `Texture::from_vec`—moving ownership of the allocation with zero pixel copying.',
            zh: 'Worker 线程将像素解码进 `Vec<u8>` 并构造 `Texture::from_vec`，直接转移内存所有权，零像素拷贝。',
            code: 'let tex: Texture<\'static> = Texture::from_vec(bytes, w, h, RGBA8888)?;',
            metric: 'Requires `features = ["sync", "std"]`',
          },
          {
            from: 1,
            to: 0,
            label: 'ImageSource::from(tex)',
            en: 'Worker finishes and wakes the task/channel. Main thread converts the `Texture` or `Handle<Texture>` into `ImageSource` and updates `Image(src: ...)`.',
            zh: 'Worker 完成后唤醒主线程轮询，主线程将其转为 `ImageSource` 并绑定到 `Image(src: ${ model.source() })`。',
            code: 'model.set_source(ImageSource::from(tex)); // triggers reactive repaint',
            metric: 'Thread-safe handoff complete',
          },
          {
            from: 0,
            to: 2,
            label: 'upload / sample cache',
            en: 'During `render_dirty()`, the GPU/SDL backend uploads the new texture revision once on the Main Thread and caches it.',
            zh: '在 `render_dirty()` 绘制阶段，渲染后端在主线程上传该纹理并记入驻留缓存。',
            code: 'paint.draw(&DrawCommand::Blit { texture: &tex, .. });',
            metric: 'Cached by Texture identity & revision',
          },
        ],
      },
      {
        en: 'Pinned CPU guard on worker',
        zh: '主线程锁定像素守卫 → 子线程并发读',
        steps: [
          {
            from: 0,
            to: 2,
            label: 'source.read_pixels()?',
            en: 'For thread-bound backends (like SDL), calling `read_pixels()` on a worker thread returns `RenderError::WrongThread`. Prepare the guard on the Main Thread first!',
            zh: '对于受创建线程约束的后端（如 SDL），在子线程直接调用 `read_pixels()` 会返回 `WrongThread`。请先在主线程获取像素守卫！',
            code: 'let pixels: TexturePixels<\'_> = source.read_pixels()?; // on Main Thread',
            metric: 'Pins CPU region before worker starts',
          },
          {
            from: 0,
            to: 1,
            label: 'scope.spawn(|| pixels.bytes())',
            en: 'Pass the borrowed `TexturePixels` guard into `std::thread::scope`. Multiple workers can read `pixels.bytes()` or `get_pixel(x, y)` concurrently without locks.',
            zh: '通过 `std::thread::scope` 将 `TexturePixels` 守卫借用给子线程。多个子线程可无锁并发读取 `pixels.bytes()`。',
            code: 'std::thread::scope(|s| { s.spawn(|| analyze(pixels.bytes())); });',
            metric: 'Zero pixel copy · No per-pixel lock',
          },
          {
            from: 1,
            to: 0,
            label: 'scope join & guard drop',
            en: 'When the scoped threads finish and `pixels` drops, the read lease is released cleanly on the Main Thread.',
            zh: '当作用域内子线程全部返回、守卫释放后，读取租约在主线程安全归还。',
            code: 'drop(pixels); // releases pinned read lease',
            metric: 'Lifetime enforced by Rust borrow checker',
          },
        ],
      },
      {
        en: 'ThreadResource<T> retirement',
        zh: '跨线程令牌释放与主线程安全销毁',
        steps: [
          {
            from: 2,
            to: 0,
            label: 'ThreadOwner::insert(raw)',
            en: 'Thread-bound backends keep raw driver textures inside `ThreadOwner` on the creating thread and hand out a transferable `ThreadResource<T>` token.',
            zh: '线程绑定后端将原生驱动纹理留在创建线程的 `ThreadOwner` 内，对外仅发放可跨线程传递的 `ThreadResource<T>` 令牌。',
            code: 'let token: ThreadResource<SdlTex> = owner.insert(raw_tex)?;',
            metric: 'Raw driver handle never leaves owner thread',
          },
          {
            from: 0,
            to: 1,
            label: 'Drop Handle<Texture> on worker',
            en: 'A worker thread drops the last `Handle<Texture>` holding `token`. Instead of calling the GPU driver from the wrong thread, `Drop` atomically marks the slot retired.',
            zh: '后台 Worker 线程丢弃了最后一个持有该令牌的句柄。此时绝不会在异线程调用图形驱动，而是以原子方式将该槽位标记为“已退役（retired）”。',
            code: 'drop(shared_handle); // marks token retired atomically',
            metric: 'Zero cross-thread driver calls',
          },
          {
            from: 2,
            to: 2,
            label: 'owner.collect() on main',
            en: 'At the next safe Main Thread maintenance boundary, `ThreadOwner::collect()` destroys all retired raw textures outside registry borrows and releases the budget receipt.',
            zh: '在主线程的下一个帧维护安全边界处，`ThreadOwner::collect()` 统一销毁所有已退役的原生纹理并归还内存预算回执。',
            code: 'owner.collect(); // destroys retired native objects on creating thread',
            metric: 'Zero allocation during collection',
          },
        ],
      },
    ],
  },
  'porting-dma-vsync': {
    badge: 'EMBEDDED ISR · DMA · VSYNC TIMELINE',
    titleEn: 'Hardware interrupts, D-Cache barriers, DMA transfer & VSync flip',
    titleZh: '硬件中断、D-Cache 内存屏障、DMA 传输与 VSync 防撕裂时序',
    mode: 'swimlane',
    lanes: [
      { en: 'Touch / Rotary ISR & Peripherals', zh: '触控 / 旋钮中断 (ISR)' },
      { en: 'UI Main Thread (mirui App)', zh: 'UI 主线程 (mirui App)' },
      { en: 'DMA Engine & Panel Scanout', zh: 'DMA 引擎与屏幕扫描 (VSync)' },
    ],
    scenarios: [
      {
        en: 'SPI DMA partial window flush',
        zh: 'SPI + DMA 局部脏矩形刷屏时序',
        steps: [
          {
            from: 0,
            to: 1,
            label: 'EXTI IRQ -> IsrRing::push',
            en: 'Touch IC pulls INT low or rotary encoder steps. ISR pushes raw sample into a lock-free SPSC ring buffer and wakes the UI thread (`sev` / RTOS semaphore).',
            zh: '触摸芯片拉低 INT 引脚或表冠旋转触发中断。ISR 将采样推入无锁 SPSC 环形队列，并唤醒 UI 主线程（`sev` 或释放 RTOS 信号量）。',
            code: 'IRQ_RING.push_from_isr(RawIrqEvent::RotaryStep(1)); cortex_m::asm::sev();',
            metric: 'ISR never touches App or World directly',
          },
          {
            from: 1,
            to: 1,
            label: 'poll_event & SwRenderer',
            en: '`app.tick()` drains `poll_event()`, flushes reactive models, solves layout, and rasterizes only the damaged `PhysicalRect` into SRAM in `RGB565Swapped` format.',
            zh: '`app.tick()` 排空 `poll_event()`、合并响应式状态、计算布局，并将变化的 `PhysicalRect` 以 `RGB565Swapped` 格式光栅化到 SRAM 帧缓冲。',
            code: 'SwRenderer::draw_request(&mut fb, dirty_rect); // writes CPU D-Cache',
            metric: 'Only damaged rows rasterized',
          },
          {
            from: 1,
            to: 2,
            label: 'Clean D-Cache -> SPI DMA',
            en: 'On Cortex-M7 / RISC-V with D-Cache, `flush(area)` cleans D-Cache for the dirty address range before starting SPI DMA so the DMA controller reads fresh SRAM bytes!',
            zh: '在带数据缓存（D-Cache）的 Cortex-M7 / RISC-V 上，`flush(area)` 先对脏区地址范围执行 `Clean D-Cache`，再发送 `CASET/RASET` 并启动 SPI DMA，防止 DMA 读到旧 SRAM 数据！',
            code: 'scb.clean_dcache_by_slice(dirty_slice); spi_dma.send_window(area, dirty_slice);',
            metric: 'Prevents cache-coherency artifacts',
          },
          {
            from: 2,
            to: 1,
            label: 'DMA TC IRQ -> begin_flush',
            en: 'While DMA streams bytes over SPI, the CPU can prepare the next tick. `begin_flush()` waits on the DMA Transfer-Complete flag before modifying the buffer again.',
            zh: 'DMA 后台推流期间 CPU 可并行处理逻辑；下一帧进入 `begin_flush()` 时等待上一轮 DMA 传输完成标志，确保不发生写冲突。',
            code: 'fn begin_flush(&mut self) -> Result<(), RenderError> { self.dma.wait_ready() }',
            metric: 'Zero tearing · Overlapped CPU & SPI bus',
          },
        ],
      },
      {
        en: 'Double-buffered LTDC / DSI + VSync',
        zh: 'LTDC / MIPI DSI 双缓冲 + VSync 翻页同步',
        steps: [
          {
            from: 1,
            to: 1,
            label: 'Draw into Back Buffer',
            en: 'Display controller continuously scans out `buf[front]`. `SwRenderer` (or DMA2D) draws this frame’s dirty rectangles into `buf[back]` without disturbing scanout.',
            zh: '显示控制器持续扫描输出前台缓冲 `buf[front]`；`SwRenderer`（或 DMA2D）将本帧脏矩形画入后台缓冲 `buf[back]`，互不干扰。',
            code: 'fn framebuffer(&mut self) -> Texture { Texture::new(&mut self.buf[self.back], ..) }',
            metric: 'Zero scanout tearing',
          },
          {
            from: 1,
            to: 2,
            label: 'end_flush() -> VSync flip',
            en: '`end_flush()` cleans D-Cache for `buf[back]`, programs the LTDC/DRM layer address for the next vertical blanking interval (VSync), and swaps `front <-> back`.',
            zh: '`end_flush()` 刷出 `buf[back]` 的 D-Cache，将 LTDC/DRM 图层地址切换挂载到下一个垂直消隐期（VSync），并交换前后缓冲下标。',
            code: 'ltdc.set_layer_address_on_vsync(self.buf[self.back].as_ptr()); self.swap();',
            metric: 'Waits once per frame at VSync',
          },
          {
            from: 2,
            to: 1,
            label: 'VSync Ack -> all_buffers()',
            en: 'Once the old front buffer is released by scanout, `FramebufferAccess::all_buffers()` exposes `[completed_buffer, released_buffer]`.',
            zh: '当 VSync 翻页完成、旧前台缓冲解除扫描占用后，`all_buffers()` 返回 `[刚画完的前台缓冲, 刚释放的后台缓冲]`。',
            code: 'fn all_buffers(&mut self) -> Vec<Texture> { vec![completed, released] }',
            metric: 'Never mirrors into an actively scanned buffer',
          },
          {
            from: 1,
            to: 1,
            label: 'mirror_buffers(shifts+rects)',
            en: '`SwRendererFactory::mirror_buffers` applies `plan.shifts` first and copies only `redraw` rects into the released buffer so both buffers are identical again.',
            zh: '`SwRendererFactory::mirror_buffers` 自动先在后台缓冲上执行 `plan.shifts` 滚动平移，再只拷贝本帧 `redraw` 脏区，使两个缓冲区恢复完全一致。',
            code: 'factory.mirror_buffers(surface, &plan, &redraw, &viewport)?;',
            metric: 'Avoids full-screen memcpy on partial updates',
          },
        ],
      },
    ],
  },
  'double-buffer-mirror': {
    badge: 'DOUBLE-BUFFER MIRRORING ORDER',
    titleEn: 'Why scroll shifts must run before dirty-rect blits on multi-buffer surfaces',
    titleZh: '双缓冲表面为何必须先执行滚动平移（shifts）再拷贝重绘矩形（redraw）',
    mode: 'pipeline',
    nodes: [
      { id: 'draw', tag: '01 · DRAW BACK', en: 'Shift & Paint Active', zh: '后台缓冲平移与重绘', sub: 'buffers[draw_index]' },
      { id: 'flip', tag: '02 · VSYNC FLIP', en: 'end_flush() Page Flip', zh: 'end_flush() 翻页呈现', sub: 'swap active <-> inactive' },
      { id: 'shift', tag: '03 · MIRROR SHIFT', en: 'Replay plan.shifts First', zh: '先在空闲缓冲重放平移', sub: 'texture_scroll_in_place' },
      { id: 'blit', tag: '04 · MIRROR BLIT', en: 'Copy redraw Rectangles', zh: '再拷贝本帧重绘脏矩形', sub: 'blit_region(inact, act)' },
    ],
    scenarios: [
      {
        en: 'Correct order (Shift → Blit)',
        zh: '正确顺序（先平移 → 后拷贝脏区）',
        steps: [
          {
            node: 0,
            en: 'When a scroll container moves by `dy = -24`, `App` shifts existing pixels inside `buffers[draw_index]` in place and repaints only the newly exposed 24 px strip at the bottom.',
            zh: '当滚动容器向上滚动 `dy = -24` 时，`App` 先在当前绘制缓冲 `buffers[draw_index]` 内原地平移既有像素，并仅重绘底部新露出的 24 px 条带。',
            code: 'plan.shifts = [ScrollShift { area, dx: 0, dy: -24 }]; redraw = [bottom_strip];',
            metric: 'Rasterized pixels: ~10% of scroll viewport',
          },
          {
            node: 1,
            en: '`end_flush()` presents `buffers[draw_index]` to the display controller at VSync. The previous front buffer becomes the writable `inactive` buffer, still holding last frame’s unshifted pixels.',
            zh: '`end_flush()` 在 VSync 边界将 `buffers[draw_index]` 切换为前台扫描缓冲；旧的前台缓冲退居为可写的 `inactive` 缓冲，但里面装的仍是上一帧未滚动的画面。',
            code: 'let [completed, inactive] = surface.all_buffers();',
            metric: 'completed = Frame N · inactive = Frame N-1',
          },
          {
            node: 2,
            en: '`SwRendererFactory::mirror_buffers` first runs `texture_scroll_in_place` on `inactive` for every shift in `plan.shifts`, moving Frame N-1’s preserved rows to their new scrolled positions.',
            zh: '`SwRendererFactory::mirror_buffers` 首先在 `inactive` 缓冲上按 `plan.shifts` 原地平移复用行，把第 N-1 帧保留下来的像素移到滚动后的新位置。',
            code: 'texture_scroll_in_place(&mut inactive, area, dx_phys, dy_phys)?;',
            metric: 'Preserved rows aligned between both buffers',
          },
          {
            node: 3,
            en: 'Finally, `mirror_buffers` copies each `area` in `redraw` (the newly exposed 24 px strip) from `completed` into `inactive`. Both buffers are now bit-for-bit identical!',
            zh: '最后，`mirror_buffers` 将 `redraw` 中的物理脏矩形（底部新露出的 24 px 条带）从 `completed` 拷贝到 `inactive`。至此两个缓冲区逐像素 100% 一致！',
            code: 'blit_region(&mut inactive, &completed, bottom_strip)?;',
            metric: 'Reversing steps 03 & 04 would smear the newly drawn strip!',
          },
        ],
      },
    ],
  },
  'compose-backend-route': {
    badge: 'HYBRID ENGINE & EXACT FALLBACK',
    titleEn: 'Per-command routing between hardware engines, SwRenderer & bounded fallback',
    titleZh: '混合后端命令路由、DMA/缓存屏障与有界软件回退流水线',
    mode: 'pipeline',
    nodes: [
      { id: 'classify', tag: '01 · ROUTE', en: 'Engine::route(request)', zh: '引擎语义与几何判定', sub: 'Native vs ExactFallback' },
      { id: 'begin', tag: '02 · BARRIER IN', en: 'Engine::begin(target)', zh: '切入硬件引擎会话', sub: 'Clean/Invalidate Cache' },
      { id: 'submit', tag: '03 · EXECUTE', en: 'Hardware or Fallback', zh: '硬件执行或局部回退重放', sub: 'DMA2D / FallbackRegion' },
      { id: 'end', tag: '04 · BARRIER OUT', en: 'Engine::end(target)', zh: '等待硬件完成并交接', sub: 'Wait TCIF before CPU' },
    ],
    scenarios: [
      {
        en: 'Hardware Native path (DMA2D / GPU)',
        zh: '硬件原生快路径 (Native)',
        steps: [
          {
            node: 0,
            en: '`Dma2dEngine::route` checks `request.validate()?`, `projective.is_identity()`, `transform().is_identity()`, and `radius == 0`. All constraints match, so it returns `Ok(RenderRoute::Native)`.',
            zh: '`Dma2dEngine::route` 校验几何无旋转、无透视且 `radius == 0`，确认外设可精确表达该矩形填充，返回 `Ok(RenderRoute::Native)`。',
            code: 'if is_axis_aligned_sharp_rect(request) { return Ok(RenderRoute::Native); }',
            metric: 'Fallback allocation: 0 B',
          },
          {
            node: 1,
            en: 'When transitioning from `SwRenderer` to `Dma2dEngine`, `compose_backend!` calls `dma2d.begin(&mut sw)` once to flush CPU D-Cache lines before hardware writes to SRAM.',
            zh: '当命令流从 `SwRenderer` 切换到 `Dma2dEngine` 时，`compose_backend!` 自动调用一次 `dma2d.begin(&mut sw)`，在硬件写 SRAM 前刷出并作废相关 D-Cache 行。',
            code: 'dma2d.begin(&mut sw_target)?; // runs once per consecutive batch',
            metric: 'Consecutive DMA2D commands share one session',
          },
          {
            node: 2,
            en: '`dma2d.submit(&mut sw, &draw)` programs the peripheral registers (`OMAR`, `OOR`, `NLR`, `ocolr`) for the physical clip intersection and starts the transfer.',
            zh: '`dma2d.submit(&mut sw, &draw)` 根据物理裁剪交集配置 `OMAR`、`OOR`、`NLR` 寄存器并启动总线填充。',
            code: 'dma2d_regs.start_fill(phys_rect, stride, packed_color);',
            metric: 'Bus-speed hardware fill',
          },
          {
            node: 3,
            en: 'When the next command routes back to `SwRenderer` (e.g. SDF text or a rounded border), `compose_backend!` calls `dma2d.end(&mut sw)` to wait for `TCIF` before CPU touches RAM.',
            zh: '当下一条命令（如圆角边框或文字）切回 `SwRenderer` 时，`compose_backend!` 自动调用 `dma2d.end(&mut sw)` 等待硬件传输完成标志（TCIF）。',
            code: 'dma2d.end(&mut sw_target); // CPU and DMA never race on RAM',
            metric: 'Deterministic barrier ordering',
          },
        ],
      },
      {
        en: 'Bounded ExactFallback scope',
        zh: '有界软件回退路径 (ExactFallback)',
        steps: [
          {
            node: 0,
            en: 'A GPU backend encounters an unsupported projective stroke or custom clip path. `route()` quantizes and clips the logical bounds once into `RenderRoute::ExactFallback(FallbackRegion)`.',
            zh: 'GPU 后端遇到不支持的透视描边或路径裁剪，`route()` 将逻辑边界一次性量化并裁剪为 `RenderRoute::ExactFallback(FallbackRegion)`。',
            code: 'FallbackRegion::from_logical_bounds(bounds, viewport, budget)?',
            metric: 'Preflight checks stride, alignment & byte capacity',
          },
          {
            node: 1,
            en: 'If the blend mode depends on existing backdrop pixels, the backend reads back only the clipped `FallbackRegion` rectangle into the reusable `ProjectiveFallback` RGBA plane.',
            zh: '若混合模式依赖背景像素，后端仅将裁剪后的 `FallbackRegion` 局部矩形回读到可复用的 `ProjectiveFallback` RGBA 平面中（不透明覆盖则直接跳过回读）。',
            code: 'if needs_backdrop { target.read_region(region, &mut fallback_plane)?; }',
            metric: 'Opaque isolated scopes skip readback',
          },
          {
            node: 2,
            en: '`SwRenderer` replays the unsupported command (or consecutive clipped scope) locally inside that bounded RGBA plane with exact subpixel anti-aliasing.',
            zh: '`SwRenderer` 在这块有界局部 RGBA 平面内重放该命令（或连续裁剪作用域），保持完全一致的亚像素抗锯齿精度。',
            code: 'sw_replay.draw_in_region(&mut fallback_plane, rebased_request)?;',
            metric: 'Reuses caller/factory scratch buffer',
          },
          {
            node: 3,
            en: 'The completed region is uploaded/composited back into the native target once. If preflight failed, zero target pixels were modified.',
            zh: '重放完毕的局部区域一次性写回原生渲染目标；若前置预算校验失败，则目标像素保持原样，绝不产生半截写入。',
            code: 'target.write_region(region, &fallback_plane)?;',
            metric: 'Atomic failure semantics',
          },
        ],
      },
    ],
  },
  'clipboard-protocol': {
    badge: 'SELECTION & CLIPBOARD REQUEST PROTOCOL',
    titleEn: 'Four-request handshake between commands.clipboard(), custom widgets & host OS',
    titleZh: '剪贴板命令、自定义可选组件与宿主操作系统的四步握手协议',
    mode: 'pipeline',
    nodes: [
      { id: 'inspect', tag: '01 · INSPECT', en: 'InspectSelection', zh: '快照选区上下文', sub: 'SelectionState::new' },
      { id: 'export', tag: '02 · EXPORT', en: 'CopySelection', zh: '导出不可变数据', sub: 'ClipboardData (MIME)' },
      { id: 'bridge', tag: '03 · HOST BRIDGE', en: 'Surface Read / Write', zh: '宿主剪贴板异步读写', sub: 'write_clipboard / read' },
      { id: 'commit', tag: '04 · COMMIT', en: 'Cut / Paste & Notify', zh: '核对上下文并原子修改', sub: 'ClipboardFinished' },
    ],
    scenarios: [
      {
        en: 'Cut lifecycle (Write-before-delete)',
        zh: '剪切（Cut）安全时序（先确认写入、后删除）',
        steps: [
          {
            node: 0,
            en: '`commands.clipboard().cut(editor)` synchronously invokes `InspectSelection` on `editor`, capturing `SelectionState::new(has_selection, revision_snapshot)`.',
            zh: '`commands.clipboard().cut(editor)` 先在目标节点上同步调用 `InspectSelection`，捕获选区存在性与版本快照 `SelectionState`。',
            code: 'SelectionState::new(!range.is_empty(), (doc_rev, sel_rev, ime_active))',
            metric: 'Empty selection finishes immediately as EmptySelection',
          },
          {
            node: 1,
            en: '`CopySelection` exports an immutable `ClipboardData` snapshot (such as `text/plain` or custom `application/x-mirui-pixel-frame`) without deleting the source yet.',
            zh: '`CopySelection` 导出不可变的 `ClipboardData` 快照（如 `text/plain` 或自定义二进制格式），此时绝不提前删除编辑器内的原内容！',
            code: 'Ok(ClipboardData::text(selected_utf8_slice))',
            metric: 'Source remains untouched while OS write is in flight',
          },
          {
            node: 2,
            en: '`Surface::write_clipboard` submits the snapshot to the host OS (synchronous on macOS/Windows/X11/Wayland, asynchronous Promise on Web Clipboard API).',
            zh: '`Surface::write_clipboard` 将快照提交给宿主系统（桌面端同步完成，Web 端通过异步 Clipboard API 等待浏览器确认）。',
            code: 'surface.write_clipboard(request) -> ClipboardDispatch',
            metric: 'Write failure preserves source text & selection',
          },
          {
            node: 3,
            en: 'After the host acknowledges the write, `mirui` re-runs `InspectSelection`: if the context still matches, `CutSelection` deletes the range and emits `ClipboardFinished(Ok(Cut))`; if the document changed mid-flight, it emits `Ok(CopiedWithoutCut)`.',
            zh: '收到宿主写入成功回执后，`mirui` 再次调用 `InspectSelection` 比对上下文：若期间文档与选区未被篡改，才调用 `CutSelection` 删除选区并派发 `Ok(Cut)`；若已变化则保留原文档并汇报 `Ok(CopiedWithoutCut)`。',
            code: 'if ctx_before == ctx_after { invoke_cut(); emit(Cut); } else { emit(CopiedWithoutCut); }',
            metric: 'Prevents deleting wrong offsets after async Web delay',
          },
        ],
      },
    ],
  },
  'target-admission': {
    badge: 'OFFSCREEN WORKING-SET ADMISSION',
    titleEn: 'How TargetPlan admits native GPU targets vs software fallback within budget',
    titleZh: '离屏渲染目标的工作集预算准入、CPU 消费者偏好与回退决策',
    mode: 'pipeline',
    nodes: [
      { id: 'plan', tag: '01 · TARGET PLAN', en: 'Storage & Readback', zh: '计算目标与回读开销', sub: 'TargetPlan::storage_bytes' },
      { id: 'workset', tag: '02 · WORKING SET', en: 'Active + Dependents', zh: '工作集峰值预算核验', sub: 'current + history + inputs' },
      { id: 'select', tag: '03 · ROUTE CHOICE', en: 'Native vs Software', zh: '选择原生 GPU 或 CPU 生产', sub: 'CpuConsumerPreferred' },
      { id: 'retire', tag: '04 · RECLAIM', en: 'Reuse or Collect', zh: '同尺寸槽位复用与回收', sub: 'ThreadOwner::collect()' },
    ],
    scenarios: [
      {
        en: 'Working-set admission check',
        zh: '工作集峰值准入与回退保护',
        steps: [
          {
            node: 0,
            en: 'Before producing an offscreen `WidgetTexture`, the renderer builds a `TargetPlan` accounting for both device texture storage and any required fallback/readback staging bytes.',
            zh: '在生成离屏 `WidgetTexture` 前，渲染器先构造 `TargetPlan`，同时计入设备纹理字节数与潜在的回读/回退暂存区开销。',
            code: 'let plan = renderer.plan_target(TargetDescriptor { width, height, alpha })?;',
            metric: 'Includes SDL-GPU / WGPU 256-byte row alignment padding',
          },
          {
            node: 1,
            en: 'Admission checks that the active target, its retained previous-frame history (`#[retain_previous]`), and all live source dependencies fit simultaneously without evicting active inputs.',
            zh: '准入检查确保当前目标、其保留的上一帧历史槽位（`#[retain_previous]`）以及所有上游依赖纹理能够同时驻留，绝不驱逐正在被依赖的活跃输入。',
            code: 'required = plan.total_bytes() + active_dependents + retained_history;',
            metric: 'Rejects before paint callback runs (WorkingSetProtected)',
          },
          {
            node: 2,
            en: 'If a downstream consumer will call `read_pixels()` on the CPU, admission selects software production up front (`CpuConsumerPreferred`) to avoid a costly GPU-to-CPU readback stall.',
            zh: '若下游消费者声明了 CPU 像素读取需求，准入器会直接选择在 CPU 上生产该离屏纹理（`CpuConsumerPreferred`），避免昂贵的 GPU 同步回读停顿。',
            code: 'if has_cpu_consumer || required > native_budget { produce_in_software(); }',
            metric: 'Traceable via `output.target` perf event',
          },
          {
            node: 3,
            en: 'Unchanged subtrees reuse their completed texture slot; resized or retired targets release their `ThreadResource` token and are destroyed at `ThreadOwner::collect()`.',
            zh: '未变化的子树直接复用已完成的离屏纹理；尺寸变化或退役的目标归还令牌，并在所属线程的 `ThreadOwner::collect()` 边界安全销毁。',
            code: 'owner.collect(); // reclaims retired GPU targets & budget receipts',
            metric: 'Zero per-frame allocation when size is stable',
          },
        ],
      },
    ],
  },
  'persistence-codec': {
    badge: 'ATOMIC RESTORE & DEFERRED SAVE STATE MACHINE',
    titleEn: 'Two-phase ModelCodec restoration, corrupt-archive protection & stroke deferral',
    titleZh: 'ModelCodec 两阶段原子恢复、损坏存档防覆盖保护与笔触暂缓状态机',
    mode: 'pipeline',
    nodes: [
      { id: 'read', tag: '01 · STORAGE READ', en: 'Fetch Raw Archive', zh: '读取原始存档字节', sub: 'Storage::read(key)' },
      { id: 'prep', tag: '02 · PREPARE', en: 'Validate & Construct', zh: '校验并构造临时值', sub: 'ModelCodec::prepare' },
      { id: 'apply', tag: '03 · APPLY', en: 'Infallible Swap', zh: '无失败原子写入模型', sub: 'ModelCodec::apply' },
      { id: 'save', tag: '04 · ENCODE & DEDUP', en: 'Encoded vs Deferred', zh: '暂缓判定与去重落盘', sub: 'WriteOutcome::Unchanged' },
    ],
    scenarios: [
      {
        en: 'Safe restore & corrupt guard',
        zh: '两阶段恢复与损坏存档防覆盖保护',
        steps: [
          {
            node: 0,
            en: 'During `app.add_model(default).finish()`, `PersistencePlugin` reads the raw bytes for `key` from `Storage` before any UI observers are attached.',
            zh: '在调用 `app.add_model(default).finish()` 时，`PersistencePlugin` 在挂载任何 UI 观察者之前先从 `Storage` 读取对应 `key` 的字节。',
            code: 'let bytes = storage.read("settings")?;',
            metric: 'Runs once at registration time',
          },
          {
            node: 1,
            en: '`ModelCodec::prepare` validates checksums, version headers, and collection bounds into a temporary value without mutating the live Model. If corrupt, it activates **restore protection**!',
            zh: '`ModelCodec::prepare` 在不触碰当前模型的前提下校验校验和、版本号与集合容量上限。若存档损坏，立即开启**防覆盖保护（restore protection）**，阻止后续自动保存覆盖旧文件！',
            code: 'let prepared = codec.prepare(&bytes)?; // Err enables restore protection',
            metric: 'Corrupt archives never half-mutate a Model',
          },
          {
            node: 2,
            en: '`ModelCodec::apply` moves the validated prepared state into the Model infallibly (zero allocation, zero fallible checks), so `App::compose` immediately sees the restored state.',
            zh: '`ModelCodec::apply` 将校验通过的临时值无失败地移入模型（此阶段零分配、无失败分支），使随后的 `App::compose` 直接读到恢复后的状态。',
            code: 'codec.apply(&mut model, prepared); // infallible',
            metric: 'All #[observe] fields initialized cleanly',
          },
          {
            node: 3,
            en: 'On autosave or `AppSuspend`, `codec.encode` can return `EncodeOutcome::Deferred` (e.g. while a drawing stroke is in progress) to postpone saving until the stroke finishes.',
            zh: '在自动保存或挂起时，`codec.encode` 可返回 `EncodeOutcome::Deferred`（例如手绘笔触尚未抬起），暂缓落盘而不报错，待笔触完成后在下一周期自动重试并按字节去重写入。',
            code: 'if self.stroke_active { return Ok(EncodeOutcome::Deferred); }',
            metric: 'Storage::write skips identical bytes (Unchanged)',
          },
        ],
      },
    ],
  },
  'resource-pipeline': {
    badge: 'COOPERATIVE ASSET & PIXEL RESIDENCY',
    titleEn: 'Metadata probing, bounded block I/O & lazy viewport decoding',
    titleZh: '元数据探测、有界分块 I/O 与视口按需像素解码状态机',
    mode: 'pipeline',
    nodes: [
      { id: 'bind', tag: '01 · DEMAND', en: 'Token / ImageSource', zh: '令牌绑定与需求订阅', sub: 'Image(src: "cover")' },
      { id: 'probe', tag: '02 · PROBE', en: 'Header Metadata Only', zh: '仅探测头部尺寸', sub: 'ProbeLoader (0 pixels)' },
      { id: 'block', tag: '03 · BLOCK I/O', en: 'Bounded AssetProvider', zh: '有界分块异步读取', sub: '4 slots · 1 KiB blocks' },
      { id: 'decode', tag: '04 · RESIDENCY', en: 'Viewport Pixel Decode', zh: '视口按需解码驻留', sub: 'poll_pixels_in (128 KiB)' },
    ],
    scenarios: [
      {
        en: 'Lazy region decode',
        zh: '按需局部解码与预算驻留',
        steps: [
          {
            node: 0,
            en: 'An `Image` widget binds a token or path. `ResourceManager` creates a demand subscription; while pending, the widget draws its fallback without blocking the frame.',
            zh: '`Image` 控件绑定令牌或路径，`ResourceManager` 建立需求订阅；在像素就绪前控件先行绘制回退占位图，绝不阻塞当前帧。',
            code: 'Image(src: ${ model.cover_token() }, width: 120, height: 80)',
            metric: 'Removing Image cancels unshared pending demand',
          },
          {
            node: 1,
            en: '`MirxLoader` runs `poll_probe` first to read the container header (`width`, `height`, `format`, `stride`) so Flexbox layout can size the widget before decoding pixels.',
            zh: '`MirxLoader` 先通过 `poll_probe` 仅读取容器头部的宽高与格式元数据，让 Flexbox 布局在解码像素前就能算出精确尺寸。',
            code: 'images.add_probed_loader(MirxLoader::new(resolver));',
            metric: 'Probes budget is separate from pixel budget',
          },
          {
            node: 2,
            en: '`AssetProvider` reads chunks into reusable `1 KiB` blocks (`AssetCacheConfig`, max 2 in-flight reads, 4 sessions) or borrows a static `mapped()` slice with zero copy.',
            zh: '`AssetProvider` 通过可复用的 `1 KiB` 块缓存分页读取外部 Flash/文件（最多 2 个在途读取、4 个会话），静态内存则直接通过 `mapped()` 零拷贝借用。',
            code: 'AssetCacheConfig::new().block_size(1024).budget(MaxSize::Bytes(8192))',
            metric: 'Cooperative work allowance: 4 KiB / 32 ops per poll',
          },
          {
            node: 3,
            en: 'When `Image` is actually inside the visible clip rect, `Texture::poll_pixels_in(region, cx)` decodes only the intersecting tiles within the `128 KiB` decode budget.',
            zh: '只有当 `Image` 真正进入可视裁剪区时，`Texture::poll_pixels_in(region, cx)` 才在 `128 KiB` 解码预算内仅解码相交的局部瓦片。',
            code: 'let region = ready!(texture.poll_pixels_in(visible_clip, cx))?;',
            metric: 'Cold unpinned tiles evict automatically under pressure',
          },
        ],
      },
    ],
  },
  'surface-sizing': {
    badge: 'VIEWPORT SIZE NEGOTIATION PROTOCOL',
    titleEn: 'Asynchronous logical size requests, host restrictions & Web sequence handshake',
    titleZh: '逻辑视口尺寸请求、宿主受限裁决与 Web 序号握手时序',
    mode: 'pipeline',
    nodes: [
      { id: 'req', tag: '01 · REQUEST', en: 'control.request_size', zh: '记录最新目标尺寸', sub: 'SurfaceSizeControl' },
      { id: 'take', tag: '02 · BOUNDARY', en: 'host.take_request()', zh: '事件边界取出请求', sub: 'Outside device borrow' },
      { id: 'host', tag: '03 · NATIVE / DOM', en: 'Window or CSS Policy', zh: '原生窗口或 DOM 策略', sub: 'sequence,width,height' },
      { id: 'ack', tag: '04 · COMPLETE', en: 'Actual or Restricted', zh: '确认尺寸与信号发布', sub: 'control.state().actual' },
    ],
    scenarios: [
      {
        en: 'Desktop (SDL / WGPU)',
        zh: '桌面窗口宿主 (SDL / WGPU)',
        steps: [
          {
            node: 0,
            en: 'Application calls `control.request_size(940, 480)`. This records a pending target and opaque sequence without resizing the window mid-frame.',
            zh: '应用调用 `control.request_size(940, 480)`。该调用仅记录待决目标与不透明序号，绝不在绘制中途直接缩放窗口。',
            code: 'let control = app.surface_size_control().unwrap(); control.request_size(940, 480)?;',
            metric: 'Zero dimensions (0×N) rejected immediately',
          },
          {
            node: 1,
            en: 'At the owner-thread event boundary (outside GPU/driver borrows), the surface calls `host.take_request()` to pop the latest request.',
            zh: '在所属线程的事件边界处（处于 GPU/驱动借用范围之外），表面调用 `host.take_request()` 取出最新请求。',
            code: 'if let Some(req) = self.size_host.take_request() { ... }',
            metric: 'Older superseded requests are automatically retired',
          },
          {
            node: 2,
            en: 'The backend requests the OS window resize. If the window is fullscreen or tiled by the window manager, the OS may clamp it to `(800, 480)` or reject it.',
            zh: '后端向操作系统请求调整窗口大小。若窗口正处于全屏或被平铺窗口管理器限制，系统可能将其钳制为 `(800, 480)` 或拒绝。',
            code: 'let actual = native_window.try_set_logical_size(req.size());',
            metric: 'Unrelated user window dragging calls `observe_size` instead',
          },
          {
            node: 3,
            en: '`host.complete(req, Ok(actual))` publishes the new `SurfaceSizeState` signal. If `actual != requested`, `state().error` reports `Restricted`.',
            zh: '`host.complete(req, Ok(actual))` 发布新的 `SurfaceSizeState` 响应式信号；若实际生效尺寸不等于请求尺寸，状态会标记 `Restricted`。',
            code: 'host.complete(req, Ok((800, 480))); // publishes Restricted if != 940x480',
            metric: 'Reactive UI bindings reading `control.state()` update on next tick',
          },
        ],
      },
      {
        en: 'Web Canvas DOM handshake',
        zh: 'Web Canvas 宿主 DOM 握手协议',
        steps: [
          {
            node: 0,
            en: 'The `<canvas data-mirui-size-host="true">` opts in. Calling `request_size(w, h)` queues the request for the next canvas maintenance boundary.',
            zh: 'HTML `<canvas data-mirui-size-host="true">` 显式开启宿主尺寸控制，`request_size(w, h)` 将请求排入下一轮维护边界。',
            code: '<canvas id="stage" data-mirui-size-host="true"></canvas>',
            metric: 'Opt-in attribute required on <canvas>',
          },
          {
            node: 1,
            en: '`WebCanvasSurface` writes `data-mirui-size-request="seq,w,h"` and dispatches a `CustomEvent("mirui-size-request")` on the canvas.',
            zh: '`WebCanvasSurface` 将 `"seq,w,h"` 写入 `data-mirui-size-request` 属性，并在 `<canvas>` 上派发 `mirui-size-request` 事件。',
            code: 'canvas.dataset.miruiSizeRequest = `${seq},${w},${h}`;',
            metric: 'Opaque monotonic sequence across app restarts',
          },
          {
            node: 2,
            en: 'The page’s JS listener applies CSS width/height (without calling back into WASM exports synchronously!) and writes `data-mirui-size-result="${seq}:ok"`.',
            zh: '页面 JS 监听器应用 CSS 宽高策略（严禁在事件处理器内同步重入调用 WASM 导出函数！），并回写 `data-mirui-size-result="${seq}:ok"`。',
            code: 'canvas.style.width = `${w}px`; canvas.dataset.miruiSizeResult = `${seq}:ok`;',
            metric: 'Synchronous DOM handler completes in the same tick',
          },
          {
            node: 3,
            en: '`WebCanvasSurface` verifies the matching sequence number, updates the physical backing buffer, and draws at the new size.',
            zh: '`WebCanvasSurface` 核对序号匹配无误后，更新画布物理像素缓冲并按新尺寸完成绘制。',
            code: 'surface.reset_size_control(); // call when replacing App on same canvas',
            metric: 'Stale async replies with old `seq` are ignored',
          },
        ],
      },
    ],
  },
  'ci-matrix': {
    badge: 'CARGO XTASK CI & GITHUB ACTIONS MATRIX',
    titleEn: 'Parallel runner groups, compile-fail contracts & summary gate',
    titleZh: 'CI 并行运行器分组、编译期诊断契约与汇总门禁拓扑',
    mode: 'pipeline',
    nodes: [
      { id: 'prep', tag: '01 · DISCOVERY', en: 'Cargo Metadata & Matrix', zh: '元数据发现与矩阵准备', sub: '.github/ci-matrix.json' },
      { id: 'static', tag: '02 · STATIC & CHA', en: 'Format, Policy & Lint', zh: '格式、Cha 策略与静态分析', sub: 'rustfmt · xrune-fmt · cha' },
      { id: 'test', tag: '03 · DUAL SUITES', en: 'Runtime & Trybuild', zh: '双配置运行时与契约', sub: 'std + linux-fb · 1.97.0' },
      { id: 'gate', tag: '04 · SUMMARY GATE', en: 'Cross Targets & Gate', zh: '交叉目标与汇总门禁', sub: 'wasm32 · riscv32 · docs' },
    ],
    scenarios: [
      {
        en: 'Check groups & contracts',
        zh: '检查分组与契约边界',
        steps: [
          {
            node: 0,
            en: '`cargo xtask ci` validates check names and discovers integration targets from `Cargo.toml` metadata (`[package.metadata.ci].contract-tests` vs runtime tests).',
            zh: '`cargo xtask ci` 校验检查项名称，并从 `Cargo.toml` 的 `[package.metadata.ci].contract-tests` 自动区分编译器契约测试与普通运行时集成测试。',
            code: 'cargo xtask ci --list',
            metric: 'Adding a new integration test requires zero CI list edits',
          },
          {
            node: 1,
            en: '`format` checks `cargo fmt` and `xrune-fmt`; `cha-policy` and `cha-analysis` validate health fixtures first, then enforce repository API and architecture rules.',
            zh: '`format` 执行 `cargo fmt` 与 `xrune-fmt`；`cha-policy` 与 `cha-analysis` 先验证健康度夹具，再执行全仓 API 策略与架构静态检查。',
            code: 'cargo xtask ci format cha-policy cha-analysis',
            metric: 'Health fixtures must trigger their expected errors',
          },
          {
            node: 2,
            en: 'Both `runtime` (unit, integration, doctests) and `contracts` (`trybuild` compile-fail `.stderr` snapshots under pinned `rustc 1.97.0`) run under `std` AND `linux-fb`.',
            zh: '`runtime`（单测、集成测试、文档测试）与 `contracts`（锁定 `rustc 1.97.0` 的 `trybuild` 编译失败诊断快照）均在 `std` 与 `linux-fb` 两套配置下各跑一遍。',
            code: 'cargo xtask ci tests-std contracts-std tests-linux-fb contracts-linux-fb',
            metric: 'Renderer unit tests: `cargo test --lib --features sdl-gpu,wgpu render::`',
          },
          {
            node: 3,
            en: '`targets` verifies `wasm32-unknown-unknown`, `riscv32imc-unknown-none-elf`, and `linux-fb`. In GitHub Actions, missing prerequisites fail hard instead of skipping.',
            zh: '`targets` 校验 `wasm32`、`riscv32imc` 裸机与 `linux-fb`。在 GitHub Actions 上，任何缺失依赖或跳过都会直接判负，只有全部分组通过才会点亮最终 `ci` 汇总门禁。',
            code: 'cargo xtask ci host-clippy examples wasm-check riscv-check linux-fb-check',
            metric: 'Pages workflow separately verifies Gallery + 20 bilingual Docs guides',
          },
        ],
      },
    ],
  },
};

const spec = computed(() => SPECS[props.kind] || SPECS['reactive-dataflow']);
const currentScenario = computed(() => spec.value.scenarios[activeScenario.value] || spec.value.scenarios[0]);
const currentStep = computed(() => currentScenario.value.steps[activeStep.value] || currentScenario.value.steps[0]);

function prevStep() {
  activeStep.value = (activeStep.value - 1 + currentScenario.value.steps.length) % currentScenario.value.steps.length;
}

function nextStep() {
  activeStep.value = (activeStep.value + 1) % currentScenario.value.steps.length;
}

function laneCenterPct(laneIndex) {
  return `${(laneIndex + 0.5) * (100 / 3)}%`;
}

function arrowSegmentStyle(st) {
  const leftLane = Math.min(st.from, st.to);
  const rightLane = Math.max(st.from, st.to);
  const leftPct = (leftLane + 0.5) * (100 / 3);
  const widthPct = (rightLane - leftLane) * (100 / 3);
  return {
    left: `${leftPct}%`,
    width: `${widthPct}%`,
  };
}
</script>

<template>
  <section class="arch-flow" :aria-label="zh ? spec.titleZh : spec.titleEn">
    <header class="arch-flow-head">
      <div class="arch-flow-title-group">
        <span class="arch-flow-badge">{{ spec.badge }}</span>
        <strong class="arch-flow-title">{{ zh ? spec.titleZh : spec.titleEn }}</strong>
      </div>
      <div v-if="spec.scenarios.length > 1" class="arch-flow-scenarios" role="tablist">
        <button
          v-for="(sc, idx) in spec.scenarios"
          :key="idx"
          type="button"
          role="tab"
          class="arch-flow-tab"
          :class="{ active: activeScenario === idx }"
          :aria-selected="activeScenario === idx"
          @click="activeScenario = idx"
        >{{ zh ? sc.zh : sc.en }}</button>
      </div>
    </header>

    <!-- Mode A: Responsive 4-Stage Pipeline Grid (Zero SVG Text Overflow) -->
    <div v-if="spec.mode === 'pipeline'" class="arch-pipeline-grid" role="list">
      <button
        v-for="(node, idx) in spec.nodes"
        :key="node.id"
        type="button"
        role="listitem"
        class="arch-stage-card"
        :class="{ active: currentStep.node === idx, visited: activeStep >= idx }"
        @click="activeStep = Math.min(idx, currentScenario.steps.length - 1)"
      >
        <div class="arch-stage-top">
          <span class="arch-stage-tag">{{ node.tag }}</span>
          <span class="arch-stage-dot" aria-hidden="true"></span>
        </div>
        <strong class="arch-stage-name">{{ zh ? node.zh : node.en }}</strong>
        <code class="arch-stage-sub">{{ node.sub }}</code>
        <span class="arch-stage-hint">{{ zh ? `阶段 0${idx + 1}` : `Stage 0${idx + 1}` }} →</span>
      </button>
    </div>

    <!-- Mode B: Responsive 3-Lane Swimlane Sequence Diagram with Opaque Pill Labels -->
    <div v-else class="arch-swimlane-board">
      <div class="arch-swimlane-headers">
        <div v-for="(lane, lIdx) in spec.lanes" :key="lIdx" class="arch-lane-header">
          <span class="arch-lane-index">0{{ lIdx + 1 }}</span>
          <strong>{{ zh ? lane.zh : lane.en }}</strong>
        </div>
      </div>
      <div class="arch-swimlane-body">
        <div
          v-for="lIdx in 3"
          :key="lIdx"
          class="arch-lane-lifeline"
          :style="{ left: laneCenterPct(lIdx - 1) }"
          aria-hidden="true"
        ></div>
        <button
          v-for="(st, sIdx) in currentScenario.steps"
          :key="sIdx"
          type="button"
          class="arch-seq-step"
          :class="{ active: activeStep === sIdx }"
          @click="activeStep = sIdx"
        >
          <template v-if="st.from !== st.to">
            <div
              class="arch-seq-connector"
              :class="{ reverse: st.to < st.from }"
              :style="arrowSegmentStyle(st)"
            >
              <span class="arch-seq-origin" aria-hidden="true"></span>
              <span class="arch-seq-line" aria-hidden="true"></span>
              <span class="arch-seq-head" aria-hidden="true"></span>
              <span class="arch-seq-pill">
                <b>0{{ sIdx + 1 }}</b>
                <code>{{ st.label }}</code>
              </span>
            </div>
          </template>
          <template v-else>
            <div
              class="arch-seq-self"
              :style="{ left: laneCenterPct(st.from) }"
            >
              <span class="arch-seq-pill self">
                <b>0{{ sIdx + 1 }}</b>
                <code>{{ st.label }}</code>
              </span>
            </div>
          </template>
        </button>
      </div>
    </div>

    <!-- Interactive Step Controls & Live Inspector -->
    <div class="arch-flow-inspector">
      <div class="arch-flow-stepper">
        <button type="button" class="arch-step-nav" @click="prevStep" :aria-label="zh ? '上一步' : 'Previous step'">←</button>
        <button
          v-for="(st, idx) in currentScenario.steps"
          :key="idx"
          type="button"
          class="arch-step-pill"
          :class="{ active: activeStep === idx }"
          @click="activeStep = idx"
        >0{{ idx + 1 }}</button>
        <button type="button" class="arch-step-nav" @click="nextStep" :aria-label="zh ? '下一步' : 'Next step'">→</button>
      </div>
      <div class="arch-flow-detail">
        <p class="arch-flow-desc">{{ zh ? currentStep.zh : currentStep.en }}</p>
        <div class="arch-flow-code-row">
          <code>{{ currentStep.code }}</code>
          <span class="arch-flow-metric">{{ currentStep.metric }}</span>
        </div>
      </div>
    </div>
  </section>
</template>
