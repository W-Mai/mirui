# 帧耗时剖析与追踪 {#timing-diagnostics}

安装时钟插件与内置的帧率汇总报告插件即可开启帧耗时统计：

```rust
use mirui::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};

app.add_plugin(StdInstantClockPlugin::default());
app.add_plugin(FpsSummaryPlugin::new(120).with_sink(|report| {
    for operation in report.operations.iter().filter(|op| op.count != 0) {
        println!("{:?}: wall {:?} ns, rate {:?}/s", operation.operation,
            operation.wall_nanos(), report.operation_rate_hz(operation.operation));
    }
}));
```

<div class="arch-diagram" role="img" aria-label="从 App 操作入口经由 FrameTimings 六阶段测量到 256 样本环形窗口与有界 Capture 追踪流的诊断流水线">
  <svg viewBox="0 0 880 184" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="td-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="186" height="144" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · OPERATION TICK</text>
    <text x="32" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">按需开启采样</text>
    <text x="32" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Tick · Update · Draw</text>
    <text x="32" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">request_timings(true)</text>
    <text x="32" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">无订阅时完全跳过时钟读取</text>
    <path d="M202 92H238" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#td-arrow)" />
    <rect x="246" y="20" width="214" height="144" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="262" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · FRAMETIMINGS (6 PHASES)</text>
    <text x="262" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">六阶段墙钟耗时测量</text>
    <text x="262" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">input · systems · layout</text>
    <text x="262" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">render · flush · seed</text>
    <text x="262" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">phase_wall_nanos() : Option&lt;u64&gt;</text>
    <path d="M460 92H496" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#td-arrow)" />
    <rect x="504" y="20" width="174" height="144" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="520" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · FRAMESTATS RING</text>
    <text x="520" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">256 样本环形统计</text>
    <text x="520" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">avg · min · max · P99</text>
    <text x="520" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">FpsSummaryPlugin</text>
    <text x="520" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">回调频率与接受提交率</text>
    <path d="M678 92H714" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#td-arrow)" />
    <rect x="722" y="20" width="142" height="144" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="738" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · CAPTURE</text>
    <text x="738" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">有界追踪事件流</text>
    <text x="738" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">128 KiB / 8 KiB</text>
    <text x="738" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Chrome JSON</text>
    <text x="738" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">满载时丢弃并记数，绝不扩容</text>
  </svg>
</div>

`FpsSummaryPlugin::new(120)` 每完结 120 次 `App` 操作输出一份汇总报告。`Tick` 涵盖输入、系统、绘制与平台维护；将更新与重绘分开投递的宿主会分别产出 `Update` 与 `Draw` 样本；直接调用 `render`、`render_dirty` 或 `snapshot` 则产出 `Draw` 样本（`Tick` 内部嵌套的绘制归入该次 `Tick`，不会重复生成样本）。

耗时采集遵循**按需启动（on-demand）**原则：安装报告插件或启动活动的 `Capture` 会自动开启采集；若未安装两者，需要读取 `FrameTimings` / `FrameStats` 资源的业务代码也可以手动开关：

```rust
app.request_timings(true);
// 通过资源 API 读取 FrameTimings / FrameStats。
app.request_timings(false);
```

当没有任何消费方请求耗时采集时，`App` 完全跳过时钟读取、记录与发布，零额外开销。

### 阶段耗时与 256 样本环形统计 {#phase-timings-and-ring-summaries}

| 阶段 / 统计项 | `FrameTimings` / `FrameStats` 访问器 | 测量范围说明 |
| --- | --- | --- |
| **`input`** | `timings.input()` | 排空 `Surface::poll_event()`、手势识别以及输入/业务通知回调执行 |
| **`systems`** | `timings.systems()` | 已注册的 `#[system]` 与 `#[timer]` 调度执行及延迟命令提交 |
| **`layout`** | `timings.layout()` | 响应式信号刷出、Flexbox/文本测量以及脏矩形收集（`dirty.layout`） |
| **`render`** | `timings.render()` | 场景遍历、离屏渲染目标生成与 `Renderer::submit` 绘制提交 |
| **`flush`** | `timings.flush()` | `Surface::begin_flush()`、`flush(PhysicalRect)`、`end_flush()` 与双缓冲镜像同步 |
| **`seed`** | `timings.seed()` | 播种上一帧视觉覆盖范围，供下一帧局部脏区差分使用 |
| **256 样本环形窗口** | `stats.summary(op, phases)` | 最近 256 次有效完结操作的均值（`avg`）、极值（`min/max`）、最近秩 `P99` 与抖动（`jitter`） |

每次操作的墙钟耗时（wall time）包含业务 `post_render` 钩子与平台维护，但不含后续的报告回调与宿主帧率等待。请注意：**墙钟耗时包含驱动阻塞与线程未被操作系统调度的时间，不能直接等同于 CPU 耗时**。

`FrameTimings` 分别记录六个核心阶段：`input`（输入处理）、`systems`（系统执行）、`layout`（布局与脏区收集）、`render`（渲染器绘制）、`flush`（表面刷新）与 `previous-coverage seeding`（上一帧覆盖范围播种）。各访问器返回 `Option<u64>`（`None` 表示该阶段未运行或无效，`Some(0)` 表示实测为 0 纳秒）。`phase_wall_nanos()` 对已测量的阶段求和。

`FrameStats::summary(operation, phases)` 从容量为 256 的环形窗口中筛选指定操作类型与阶段组合，返回样本数、均值、最小值、最大值、最近秩 P99 与抖动（jitter）。`operation_rate_hz` 统计半开时间区间内的操作回调频率，而 `accepted_submission_rate_hz()` 则统计实际产生并被 `App` 接受的绘制提交频率。

## 有界追踪流 (`PerfReportPlugin` 与 `Capture`) {#bounded-traces}

`PerfReportPlugin` 持有追踪事件流并在每次操作完结后排空已注册的生产者。`with_trace_budget(bytes)` 在安装前设定每个生产者的诊断内存预算（`std` 默认 `128 KiB`，`no_std` 默认 `8 KiB`）。当缓冲区写满时，记录器会直接丢弃新记录并递增丢包计数器，**绝不会在运行时动态扩容缓冲区**。

也可以直接使用 `Capture` 手动捕获并消费追踪流：

```rust
use mirui::core::perf::{self, Capture, DEFAULT_TRACE_BUDGET};

let mut capture = Capture::new(DEFAULT_TRACE_BUDGET)?;
{
    let _span = perf::enter("assets.decode");
    decode_assets();
}
capture.drain_events(|batch, events| {
    // 在回调返回前借用事件批次；空批次同样携带轨道状态。
    consume(batch, events);
});
capture.stop();
capture.drain_events(|batch, events| consume(batch, events));
```

同一时刻全进程只能有一个活动的 `Capture`（重复创建返回 `CaptureError::InUse`）。主捕获线程在构造时自动注册；其他工作线程在打点前需调用一次 `perf::register_thread()`。`Capture::drain` 会聚合包含性墙钟总计（inclusive wall totals）并可输出按行分隔的 Chrome Trace JSON 记录；`Capture::drain_events` 则直接借用原始事件切片而不执行 JSON 编码。

启用 `perf` 特性后，`lifecycle.prepare` 会进一步细分为 `reactive.flush`（内含回收、回调批处理与脏标记）、挂载维护与视图协调，帮助区分轮询边界处的耗时究竟来自业务回调还是输入法/后端同步。

## 操作、提交与显示反馈关联 {#operations-submissions-and-feedback}

同一个有界事件流中除了普通耗时区间（span）外，还包含操作上下文、提交尝试（`perf::submission`）、等待关联（`perf::wait`）、`app.accepted` 确认以及原生硬件反馈（`perf::feedback` / `perf::display_presented`）：

```rust
let attempt = perf::submission("driver.submit");
let token = attempt.id();
match driver.submit() {
    Ok(request) => {
        pending.attach_trace(request, token);
        attempt.finish(perf::SubmissionOutcome::Issued);
    }
    Err(error) => {
        attempt.finish(perf::SubmissionOutcome::Failed);
        return Err(error);
    }
}

// 在后续帧操作中，将等待耗时关联回上面那次具体的提交 token：
let _wait = perf::wait("driver.wait_previous", pending.trace_id());
driver.wait_previous()?;
```

通过 `waits_on` 令牌关联，诊断工具可以明确分辨出当前帧里的长耗时等待究竟是在等哪一帧的 GPU 提交或翻页事件。

## 渲染后端证据探针 (`Backend evidence`) {#backend-evidence}

启用 `perf` 特性后，渲染后端会向有界追踪流写入精细的决策与传输证据：
- **`output.target`**：记录每次离屏输出决策（`Unchanged` 复用已完成像素、`Reused` 复用可写存储、`Allocated` 新建存储、`CpuConsumerPreferred` 因存在 CPU 消费者而主动选择 CPU 生产，或 `Budget` / `WorkspaceUnavailable` / `WorkingSetProtected` 等拒绝原因）以及当時の `available_bytes` 剩余预算；
- **`sdl.target.workspace`**：单独汇报因 SDL 目标回退工作区不足而导致的原生目标拒绝；
- **离屏与传输区间**：`draw.offscreen.software`、`draw.offscreen.native`、`sdl.texture.upload(.bytes)`、`sdl.target.readback(.bytes)` 与 `sdl.target.writeback(.bytes)`；
- **Linux DRM 翻页反馈**：将追踪令牌挂载在待决的 page flip 上，待收到内核 CRTC 翻页完成事件时记录硬件时钟域（`drm.monotonic` 或 `drm.realtime`）与刷新序号。

## 线程 CPU 耗时采样 {#thread-cpu-time}

调用 `FpsSummaryPlugin::with_thread_cpu_time(true)`、`PerfReportPlugin::with_thread_cpu_time(true)` 或 `capture.set_thread_cpu_time(true)` 可在操作边界与提交/等待作用域处额外采集当前线程的实际 CPU 执行时间（Linux / Android / macOS / iOS 使用 `CLOCK_THREAD_CPUTIME_ID`，Windows 使用 `GetThreadTimes`；默认关闭）。

## 诊断观察者自身开销 (`Observer cost`) {#observer-cost}

为了防止性能分析工具自身的开销掩盖真实问题，追踪流会单独通过 `probe.aggregate`、`probe.encode`、`probe.sink`、`probe.drain` 与 `probe.report` 记录排空聚合、JSON 编码与报告回调所消耗的墙钟时间，并在下一次排空时交付。
