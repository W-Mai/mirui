# 共享纹理 {#shared-textures}

可选的 `sync` 特性使 `Texture`、其像素与区域守卫、字节源，以及当 `T: Send + Sync` 时的 `cache::Handle<T>` 具备跨线程所有权。该特性默认关闭，且不会启用 `std`。目标平台需要指针宽度的原子操作；裸机应用需提供各自平台的 `critical-section` 实现。本地构建仍保留 `Rc`、局部状态与回调。

```toml
# 在既有的 mirui 依赖中：
features = ["sync", "std"]
```

## 工作线程移交 {#worker-handoff}

把已有的解码缓冲区移入常规纹理构造函数。移动纹理会一并移交原本的像素分配。共享已完成的资源则使用其现有的值句柄。

```rust
use mirui::core::cache::Handle;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::RenderError;

fn finish_decode(bytes: Vec<u8>, width: u16, height: u16)
    -> Result<Texture<'static>, RenderError>
{
    Texture::from_vec(bytes, width, height, ColorFormat::RGBA8888)
}

fn start_worker(bytes: Vec<u8>, width: u16, height: u16)
    -> std::thread::JoinHandle<Result<Texture<'static>, RenderError>>
{
    std::thread::spawn(move || finish_decode(bytes, width, height))
}

fn inspect(value: Handle<Texture<'static>>)
    -> std::thread::JoinHandle<Result<usize, RenderError>>
{
    std::thread::spawn(move || Ok(value.read_pixels()?.bytes().len()))
}
```

现有加载器通过任务唤醒与轮询，在所属 App 线程上发布工作线程的完成结果。`ResourceHandle::get()` 返回已完成或回退的 `Handle<Texture>`；把这个值句柄发送给工作线程即可。App、World、Models、Signals、资源管理器与按需句柄仍各自保持属主线程契约。`sync` 不会让渲染器或窗口事件循环变成可并发调用。

`Image::new(texture)` 与 `Image::new(handle)` 通过标准的图像准备与合成流程显示自有纹理和既有值句柄。`ImageSource::from` 对两者都接受，可用于 `prop!(ImageSource)` 输入与响应式 `src` 绑定。在 `sync` 下 `ImageSource` 满足 `Send + Sync`；其默认的共享句柄表示仍为本地所有权。访问原生源保留相同的后端与线程限制。`App::take_widget_output` 连同原始预算凭据移出一个独立的已完成输出，所得纹理可以直接喂给同一图像输入。

纹理仍不可 `Clone`。克隆值句柄保留同一个不可变值；快照则显式复制像素。借用纹理与守卫保留其 Rust 生命周期。作用域线程可以借用就绪的守卫，而无需复制像素。

## 原生源 {#native-sources}

兼容的同设备绘制直接采样原生存储。其他消费者通过源准备所需的 CPU 区域，再使用其既有上传缓存。准备阶段会在提交之前固定该区域；逐像素迭代既不会访问设备，也不会按像素加锁。

SDL 原生纹理停留在其创建线程。其可移交表示由元数据与属主令牌组成。在另一线程上读取缺失的 CPU 区域会返回 `RenderError::WrongThread`，而不会排队任何设备操作。请先在源线程上准备好像素，再交给工作线程：

```rust
use mirui::render::texture::Texture;
use mirui::render::RenderError;

fn inspect_on_worker(source: &Texture<'_>) -> Result<usize, RenderError> {
    let pixels = source.read_pixels()?;
    Ok(std::thread::scope(|scope| {
        scope.spawn(move || pixels.bytes().len()).join().unwrap()
    }))
}
```

关闭 SDL 表面会使原生源失效，并在源线程上释放裸目标。已准备的 CPU 守卫与驻留区域仍然可读；未覆盖的区域返回源错误。工作线程释放令牌只是标记退役，实际销毁由既有的属主维护完成。本地构建通过源所有权保留既有的设备生命周期。

原生 WGPU 资源共享其可移植的设备上下文，并串行化可复用的回读状态。即使原始表面已被丢弃，纹理仍可保留该上下文。设备故障与源/目标冲突沿用各自的常规错误。Web 后端保持既有的 CPU 输出策略；`sync` 不提供 JavaScript 对象移交协议，也不会创建浏览器工作线程运行时。源的约束遵循各目标绑定所提供的 Rust 自动 trait；仅仅某个类型被接受，并不能构成对外的 JavaScript 工作线程移交。

## 源、预算与扩展 {#sources-budgets-and-extensions}

在 `sync` 下，`AssetProvider` 与 `AssetReader` 要求 `Send + Sync`；其 open 与 read future 要求 `Send`。轮询仍保持独占。稳定的映射保留其原始读取器；挂起或已取消的读取会一直保留在途缓冲区，直到其物理属主释放。加载器完成时唤醒既有的 UI 需求，而不是从工作线程访问 UI 状态。

源数据块、解码像素、输出存储与后端上传缓存各自保持独立的预算。淘汰索引或关闭属主并不会释放那些仍被持有的像素或控制存储的计费。准备阶段使用有界等待者，不建立无界工作队列。共享元数据计入相应的既有预算。

在此特性下，`BackendTarget` 载荷要求 `Send + Sync`。可移植后端直接保留其线程安全的设备存储。线程绑定后端使用 `ThreadOwner` 与 `ThreadResource<T>`，而不让裸设备对象变得可移交。原生目标把 `TargetStorage` 挂到实际的存储属主上。既有 `compose_backend!` 路由与目标绑定保持不变。扩展契约见[后端目标存储](backend-targets.md)。

## 索引图像 {#indexed-images}

`Texture::from_mirx` 接受 MIRX I1、I2、I4 与 I8 图像。软件渲染直接读取这些紧凑采样，并在图像调色板中查找直通 alpha 的 RGBA 颜色。GPU 后端在上传时解析所请求的区域。多个精灵可以共享同一图集。

`TexturePixels::get_pixel` 会采样所有受支持的编码。直接消费底层字节的代码可用 `direct_format()` 选择直通颜色快路径。对于索引视图，`sample_format()`、`indexed_color_table()`、`stride()` 与 MSB 优先的 `bit_offset()` 用来描述这些采样。`as_texture()` 借用视图，并完整保留其调色板与裁剪原点。可变纹理目标使用直通颜色格式。
