# 视口内容尺寸控制 {#surface-content-sizing}

`Surface::size_control()` 按需提供逻辑内容尺寸请求能力。固定分辨率显示屏返回 `None`。获取尺寸控制器本身绝不能改变当前的内容尺寸。

<ArchFlow kind="surface-sizing" />

## 请求与观察视口尺寸 {#requesting-and-observing-surface-size}

`SurfaceSizeControl::request_size(width, height)` 记录最新的目标尺寸。成功记录请求并不代表宿主窗口已经完成尺寸调整。`state()` 通过现有的响应式信号机制观察实际尺寸、待决目标以及宿主的拒绝状态；`state_untracked()` 在不建立响应式订阅的前提下读取同一份快照。零尺寸请求会被直接拒绝。当宿主被释放后，仍被持有的控制器会拒绝后续的新请求。

| 字段 / 变体 | 类型 | 含义 |
| --- | --- | --- |
| `actual` | `(u16, u16)` | 宿主当前汇报的实际生效逻辑内容尺寸 `(width, height)`。 |
| `pending` | `Option<(u16, u16)>` | 已记录但尚待宿主事件边界执行确认的最新目标尺寸。 |
| `error: None` | `Option<SurfaceSizeError>` | 最近一次请求已按目标尺寸顺利完成（或尚未产生错误）。 |
| `error: Some(Unsupported)` | `SurfaceSizeError` | 当前宿主或显示模式不支持程序化调整内容尺寸。 |
| `error: Some(Rejected)` | `SurfaceSizeError` | 宿主拒绝了本次请求（例如零尺寸、处于全屏/最大化锁定状态，或宿主已释放）。 |
| `error: Some(Restricted)` | `SurfaceSizeError` | 宿主调整了尺寸，但受窗口限制对最终尺寸进行了裁剪或约束（`actual != requested`）。 |

各渲染后端内部保留一个 `SurfaceSizeHost` 并返回其 `control()`。仅在调用方首次请求尺寸控制时惰性创建该宿主。在现有的所属线程事件边界处，后端取出最新请求，在设备借用范围之外调用原生窗口缩放操作，并以实际逻辑尺寸或 `SurfaceSizeError` 完成本次请求。异步实现会一直保留不透明的 `SurfaceSizeRequest`，直到收到宿主反馈。已被新请求取代的旧完成通知不能用于确认较新的请求。对于与主动请求无关的原生窗口缩放，后端通过 `observe_size` 发布新尺寸；该调用不会完结正在等待的请求操作。

```rust
use mirui::surface::{SurfaceSizeError, SurfaceSizeHost};

let host = SurfaceSizeHost::new(640, 480);
let control = host.control();
control.request_size(940, 480)?;

if let Some(request) = host.take_request() {
    let (width, height) = request.size();
    // 在宿主线程执行调整，并汇报实际生效的内容尺寸。
    host.complete(request, Ok((width, height)));
}
assert_eq!(control.state().actual, (940, 480));
# Ok::<(), SurfaceSizeError>(())
```

当实际生效的尺寸与请求尺寸不一致时，状态会报告 `Restricted`。宿主也可以在不改变现有内容的前提下直接拒绝不可用或不受支持的显示模式。原生帧缓冲与附件的变更依然沿用后端现有的视口与重设尺寸路径；尺寸控制本身不会创建渲染目标，也不会重映射输入坐标。

## 原生窗口化宿主 {#native-windowed-hosts}

SDL、SDL-GPU 与桌面端 WGPU 为窗口化宿主暴露了尺寸控制能力。原生缩放操作在现有的事件边界处执行；处于最大化或全屏模式的窗口可以拒绝尺寸请求。`SlowSurface` 会透过嵌套包装层原样转发同一个控制器。异步宿主在遇到临时中断时可以调用 `defer(request)` 暂缓执行；只有当前最新的请求才能退回待决槽位。

## Web Canvas DOM 握手协议 {#web-canvas-dom-handshake}

Web Canvas 宿主通过声明 `data-mirui-size-host="true"` 显式启用尺寸控制。画布表面会将 `sequence,width,height` 写入 `data-mirui-size-request` 属性，并在 `<canvas>` 元素上派发 `mirui-size-request` 事件。文档宿主随后应用自身的 CSS 尺寸策略，并回写 `data-mirui-size-result="sequence:ok"` 或 `"sequence:rejected"`。若事件处理器仅执行同步 DOM 操作，画布表面就能在同一轮操作内确认新几何尺寸并直接按新尺寸绘制。异步处理器在修改几何尺寸或写回结果前，必须先核对请求属性中的序号是否仍然匹配。该序号是不透明标识，即使替换应用实例也会保持唯一。切勿在该事件处理器内同步重入调用 App 或 WASM 导出函数。当宿主需要替换应用实例并复用同一个 `<canvas>` 时，可先调用 `WebCanvasSurface::reset_size_control()` 退役旧请求与旧句柄。
