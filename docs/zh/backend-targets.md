# 各平台渲染目标存储 {#backend-target-storage}

`mirui::render::BackendTarget` 描述原生离屏输出存储（native output storage）与数据源持有的 CPU 回读（readback）能力。它是面向渲染后端开发者的扩展接口；上层应用与自定义控件则统一通过 `Texture` 与 `Outputs` 消费离屏输出。

当启用可选的 `sync` 特性时，目标载荷必须实现 `Send + Sync`。对于受创建线程约束的原生图形对象（如 SDL 纹理），后端将其保留在所属线程所有者内部，仅跨线程传递轻量令牌与元数据；默认特性集则直接保留线程本地载荷。关于应用层的跨线程传递，请参阅[跨线程纹理移交](shared-textures.md)。

## 存储规划与生命周期 {#storage-and-lifetime}

`TargetDescriptor` 将经过校验的 `PlaneLayout` 与通道透明度编码（`TargetAlpha::Straight` 直通 Alpha 或 `Premultiplied` 预乘 Alpha）组合在一起。`TargetAlpha` 描述存储在纹理中的通道编码，独立于绘制目标合成时使用的 `AlphaMode`。

`TargetPlan` 汇报原生存储字节数、绘制工作区字节数以及 `readback_bytes`（用于完整回读源目标所需的后端中转缓冲大小，不含最终存放结果的 CPU 像素切片）。SDL 直接写入目标切片，因此汇报 `0`；WGPU 则汇报按行对齐的 staging buffer 大小并在分配前校验设备缓冲区上限。`allocated_bytes()` 汇报目标实际占用的字节数。

控件离屏输出准入（admission）在优先尝试为原生消费者分配原生目标之前，会先在现有字节预算内保护其余待决输出的保守像素需求与共享绘制工作区。当某个待决源同时存在 CPU 消费者时，框架会优先选择直接生产 CPU 像素，再由原生消费者走既有的纹理上传缓存路径消费该像素，从而避免无谓的 GPU 回读。

在启用共享存储时，生产者会在分配完成且发布之前调用 `BackendTarget::retain_storage(TargetStorage)` 挂载一张存储回执（receipt），用于统一追踪存储配额与回读预留，直到关联的底层资源与延迟销毁全部完成才释放回执。

### 线程绑定资源 (`ThreadOwner` 与 `ThreadResource<T>`) {#thread-bound-resources}

同时启用共享存储与 `std` 时，受线程约束的后端可以使用 `render::target::ThreadOwner` 与 `ThreadResource<T>`。哪怕底层原生句柄 `T` 不是线程安全的，`ThreadResource<T>` 令牌依然可以跨线程传递：
- 在创建线程上调用 `with` 可解析出原生资源引用；
- 在其他线程上调用返回 `RenderError::WrongThread`（绝不会向远端线程隐式排队设备调用）；
- 若所有者已关闭则返回 `InvalidTexture`；递归重入访问则返回 `InvalidScope`。

`collect` 在注册表借用之外无分配地销毁已退役的原生对象，通常在帧维护等安全边界处调用；`close` 则先使所有令牌失效，再销毁空闲资源。

### SDL-GPU 原生渲染目标 {#sdl-gpu-targets}

SDL-GPU 表面与上传缓存共享同一个渲染器/窗口所有者与资源代数（generation）。临时渲染器仅在实际上传、绘制或回读期间短暂借用底层驱动状态；源像素准备、CPU 字形光栅化、软件回退与区域编辑回调均在该借用之外运行。

SDL-GPU 原生目标采用单重采样、紧凑排列的预乘 RGBA8 存储（窗口级 MSAA 不会给离屏目标纹理附加多重采样附件）。`Rebuild` 重建会清空整张目标；`Incremental` 增量作用域则保留已有的有效内容。当驱动支持预乘混合且合成模式为 `SourceOver` 时，同一设备上的 SDL 消费者可以直接采样已完成的原生目标纹理；跨设备、不支持的混合模式或其他合成模式则通过源端回读像素与上传缓存完成绘制。

### WGPU 原生渲染目标 {#wgpu-targets}

WGPU 帧附件决定渲染管线格式、采样数、裁剪边界与回读格式。原生 WGPU 目标独立于窗口表面与工厂持有原始设备上下文（Device / Queue），并在增量作用域之间保留多重采样附件与解析视图（resolved view）。

同一设备上的 WGPU 消费者直接通过解析视图采样已完成的原生目标，无需生成 CPU 像素或占用上传缓存。跨设备或需要 CPU 访问的消费者则复用设备专属的 staging buffer 与映射状态执行回读，将源字节直接转换为经过校验的直通 Alpha 像素存储。

## 源目标回读 (`Source reads`) {#source-reads}

原生目标的 CPU 回读（`read_target_pixels`）在首次需要 CPU 像素时按需拉取并缓存在源端。尚未完成写入或仍处于活动绑定状态的目标会被拒绝回读。回读完成后，多个 CPU 消费者共享同一份缓存像素；只有在目标被重新写入并推进修订号后才会触发下一次回读。

### 局部区域回读与 CPU 缓存 {#region-readback-and-cpu-cache}

当局部区域编辑（region edit）或软件复合回退只需要目标的一小块裁剪矩形时，SDL-GPU 与原生 WGPU 会直接将该矩形区域回读到调用方提供的目标切片中，无需先分配整张全尺寸的中间 CPU 纹理。未覆盖的像素与行尾填充保持原样不变。

### 离屏输出准入与发布 {#output-admission-and-publication}

离屏输出预留会针对目标生产者与 `TargetPlan` 校验待写入槽位的兼容性：兼容的存储直接原地复用；不兼容的存储在现有预算内释放并重建（不会破坏已冻结的历史帧槽位）。正在被读取的输出会钉住（pin）其底层存储，内存修剪（`trim_memory`）会自动跳过正在被借用的输出像素。

## 渲染器目标作用域 (`Renderer scopes`) {#renderer-scopes}

SDL-GPU 与原生 WGPU 在规划原生目标时，要求工厂已配置足够容纳整张目标尺寸回退的目标工作区（SDL 检查 `with_projective_fallback` 容量，WGPU 检查 `with_target_edit_budget` 与设备回读上限）。例如为 `640×480` 的紧凑 RGBA8 目标预留 SDL-GPU 工作区：

```rust
use mirui::render::{PlaneLayout, ProjectiveFallback};
use mirui::render::texture::ColorFormat;
use mirui::surface::sdl_gpu::SdlGpuFactory;

let layout = PlaneLayout::packed(640, 480, ColorFormat::RGBA8888).unwrap();
let factory = SdlGpuFactory::new().with_projective_fallback(
    ProjectiveFallback::new(vec![0; layout.required_bytes()])
);
// 随后将 factory 传给 App::with_factory(surface, factory)。
```

每个 `Renderer` 声明关联类型 `type TargetState`（不支持原生离屏目标的渲染器设为 `()`），并实现 `plan_target`、`can_reuse_target`、`create_target`、`bind_target` 与 `restore_target`。默认的 `with_target(target, viewport, update, workspace, body)` 执行器会先刷出父目标的待决绘制、校验并绑定子目标、执行一次绘制闭包、刷出子目标命令，最后恢复父目标状态（即使回调报错或展开也会确保父目标恢复或将设备标记为失效）。

### 组合后端一致性契约 (`Conformance`) {#conformance}

`compose_backend!` 将目标规划、复用检查、创建、绑定与恢复自动转发给其 `default` 字段，并复用统一的作用域执行器。独立的离屏目标使用各自的局部视口与局部坐标系；父渲染器上的区域偏移与透视变换不会在生产离屏图像期间扭曲子目标，而是在将该成品图像合成回父目标时才应用。

仓库中的[组合目标契约测试](../../tests/compose_backend_basic/native_targets.rs)与[增量更新契约测试](../../tests/compose_backend_basic/native_targets/updates.rs)使用内存模拟的原生设备完整验证了目标规划、双层嵌套引擎、失败回滚与跨帧存活读取，无需依赖物理 GPU 硬件即可在 CI 中运行。

## SDL 帧追踪探针 (`SDL frame tracing`) {#sdl-frame-tracing}

启用 `perf` 特性后：
- `output.target` 记录每次离屏输出是选择了原生目标还是 CPU 软件生产、是否复用了已有存储以及准入被拒的具体原因；
- `sdl.target.workspace` 独立标识因 SDL 目标回退工作区缺失或容量不足而导致的回退；
- `sdl.target.select`、`sdl.target.restore`、`sdl.frame.compose`、`sdl.queue.submit` 与 `sdl.present` 分别测量目标切换、窗口合成、命令提交与上屏呈现的调用线程墙钟耗时（关于字段含义与归因规则，请参阅[帧耗时剖析与追踪](timing-diagnostics.md#backend-evidence)）。
