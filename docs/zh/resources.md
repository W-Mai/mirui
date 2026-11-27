# 静态与托管资源池 {#resources}

`ResourceManager<T>` 在既有缓存预算下将资源令牌（token）解析为共享值。`ResourceHandle::get()` 与 `ResourceManager::resolve()` 返回已完成值或管理器回退值（fallback）的 `cache::Handle<T>`，直接共享同一缓存条目而不拷贝底层内容。已获取的值句柄在条目被替换、驱逐乃至管理器被销毁后依然可读；而通过需求句柄（`ResourceHandle`）发起的下一次读取则会自动跟随管理器的当前最新值。静态注册项与已完成句柄的读取不会触发加载器（loader）。

<ArchFlow kind="resource-pipeline" />

`Image(src: ${expression})` 通过现有的响应式属性机制绑定资源令牌、矢量路径或已驻留纹理，并可通过 `prop!(ImageSource)` 在多层组合函数之间转发。切换图像源会保留控件原有的绘制配置、释放上一个资源的需求订阅并转而订阅新资源；相等的值不会触发无谓更新。强类型构建器使用 `bind_property::<prop::ImageSource>(getter)`，回调中则可使用 `commands.set::<prop::ImageSource>(entity, source.into())`。

`Image::new(texture)` 与 `Image::new(handle)` 直接接收自有 `Texture<'static>` 或现有的 `cache::Handle<Texture<'static>>`；`ImageSource::from` 同样支持将它们转为响应式输入。直接句柄独立于缓存索引驱逐或替换展示其所持有的像素，无需经过资源管理器或生成令牌：

```rust
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::ui::widgets::{Image, image::ImageSource};

#[compose]
fn picture(source: prop!(ImageSource)) -> Entity {
    ui! { Image(src: ${ source }, width: 32, height: 32) }
}

let texture = Texture::owned(32, 32, ColorFormat::RGBA8888).unwrap();
let source = ImageSource::from(texture);
let mut app = App::headless(32, 32);
app.with_default_widgets();
let root = app.spawn_root().id();
app.compose(root, picture(source));
```

`ImageSource` 字段支持直接在 `#[model]` 中标注 `#[observe]`，其生成的读取方法可直接喂给 `Image(src: ${model.source()})` 或转发给 `prop!(ImageSource)`（资源令牌与定点矢量命令按值比对，纹理句柄按共享缓存条目身份比对）。

## 资源元数据探测 (`ProbeLoader`) {#metadata}

启用可选的 `sync` 特性后，纹理、不可变值句柄与字节源可以跨线程传递，而资源管理器与需求句柄仍留在所属的 `App` 线程上（详见[跨线程纹理移交](shared-textures.md)）。

`Loader<T>` 负责产出解码后的完整资源值；而 `ProbeLoader<T>` 则支持**在不解码像素的前提下先行探测图像宽高与格式元数据**。通过 `manager.add_probed_loader(loader)` 注册探测加载器（普通 `add_loader` 不会隐式开启探测）：

```rust,ignore
app.add_plugin(
    ImageResourcesPlugin::empty().with_loader(|images| {
        images.add_probed_loader(MirxLoader::new(|token| match token {
            "cover" => Some(include_bytes!("assets/cover.mirx").as_slice()),
            _ => None,
        }));
    }),
);
```

`MirxLoader` 无需解码像素即可读取 MIRX 图像头部的尺寸元数据。通过 `with_probes` 或 `enable_probes` 可启用元数据缓存预算；当令牌被替换、移除或完成完整加载时，缓存的探测元数据会自动失效。

## 回调生命周期与重入防护 {#callback-lifetime}

加载器与元数据探测器回调在执行期间**不持有资源管理器的借用锁**。如果在某个令牌自己的加载器内部递归解析同一个令牌，会直接返回回退值而不会造成死锁或无限递归；与此同时，加载器内部仍可正常解析其他依赖令牌。若在加载期间该令牌被替换或移除，旧调用的完成结果会被自动作废，绝不会覆盖新注册的值。缓存命中与已预热的句柄读取不分配调用守卫，也不触发外部加载器。

共享存储在一个统一预算下以事务方式协调预留、分配与驻留缓冲区记账。丢弃缓存索引不会释放仍被值句柄或像素守卫持有的存储；加载器执行与令牌观察始终留在所属 `App` 线程上。

## 协作式异步加载 (`Loader` 与 `poll_pending`) {#deferred-loading}

`Loader` 提供 `State`、`recognize`、`poll_load` 与 `reset`；`ProbeLoader` 在同一状态上提供 `poll_probe`。尚未完成的挂起调用会保留当前进度并向消费方返回回退值。加载器必须将 `LoadContext::waker()` 注册到其完成通知机制上，并通过 `LoadContext::consume_work()` 统计协作式解码步数（配额耗尽时自动让出并调度下一轮轮询）。

所属线程执行器调用 `manager.poll_pending(&mut context)` 推进就绪调用并通过既有令牌订阅发布已完成的值。外部 I/O 唤醒绝不会跨线程直接调用加载器或改写 UI 状态。每个加载器在注册时预先准备 **4 个可复用的并发调用槽位（invocation slots）**；槽位满载时新请求返回回退值并标记为失败（可通过 `clear_failed(token)` 显式重试）。

`manager.reload(token)` 将已注册的加载器链排入下一轮所属线程轮询，并在新值加载成功前保留缓存中的旧值（刷新失败不会丢弃旧值）。

## 图像控件响应式更新 {#image-updates}

图像加载器在所属线程的内置系统中自动推进。移动端宿主将完成唤醒接入系统事件循环；桌面端、Web 与自驱动嵌入式宿主在下一帧 tick 推进就绪任务。挂起的加载任务不会无故空转请求动画帧。

`Image(src: "cover", ...)` 通过已注册的 `View` 观察其资源令牌：当该令牌在资源管理器中被替换、重载完成或移除时，图像会自动失效重绘；切换 `src` 或销毁实体则会立即释放对旧令牌的加载需求（若其他 `Image` 仍在请求同一令牌，则共享的在途加载继续进行）。

## 字节源提供者与按需区域解码 (`AssetProvider` & `poll_pixels_in`) {#byte-providers}

| 预算层级 / 配置项 | 管控范围与计费对象 | 默认值 / 典型上限 | 容量耗尽与淘汰行为 |
| --- | --- | --- | --- |
| **`values` 预算** (`ResourceManager<T>`) | 已完成的资源缓存条目（`cache::Handle<T>`）、内存映射 RAW 长度及纹理控制存储 | 按 `ResourceManager` 配置（`MaxSize::Bytes` 或 `Count`） | 淘汰未被引用的冷索引条目；已被持有的 `cache::Handle<T>` 在释放前始终可读 |
| **`probes` 预算** (`with_probes` / `enable_probes`) | 仅探测不解码的元数据（`ProbeLoader<T>`、图像宽高与格式头） | 可选元数据缓存（`MaxSize::Bytes` 或 `Count`） | 令牌替换、移除或完整加载完成时自动失效；未启用缓存时仍直接返回探测结果 |
| **`AssetCacheConfig`** (`ResourcePlugin::with_cache`) | 共享源字节块、被读取器借用的已驱逐块以及在途物理读取缓冲 | `1 KiB` 块、`4 KiB` 预算、`2` 个在途读取、`4` 个会话/提供方、`1 KiB` 键长、`256 B` 识别前缀 | 拒绝无界源缓存；会话或读取槽位耗尽时返回 `Capacity` 而不隐式扩容 |
| **`with_decode_budget(bytes)`** (`ImageResourcesPlugin`) | 内置源解码器共享活动池：局部区域瓦片、在途区域读取、被锁定的已驱逐像素、编码组元数据与编解码工作区 | 默认按需准备 `128 KiB`（静态与 `mapped()` RAW 图像零占用） | 优先复用未锁定的冷瓦片；被锁定的活跃区域占满预算时返回 `ResourceLimit(Texture)` |

### `AssetProvider` 与零拷贝内存映射切片 {#asset-provider-and-mapped-slices}

当基于字节源的 `Texture` 发布后，其保留的读取进度归属于该纹理值本身，而非单个 `Image` 控件。移除或切换 `Image` 仅停止驱动旧像素需求，不会清除已缓存的成功值或令其他独立的纹理持有者失效；释放最后一个纹理持有者时，才会通过原始提供方会话安全退役在途物理读取。每个物理读取最多保留 8 个不同的等待上下文（同一 `App` 内的多个控件共享同一个唤醒槽位）。

`AssetProvider` 按键打开对象，其关联的 `AssetReader` 汇报长度、定位与内容一致性（`SourceConsistency::Stable` 保证跨多次读取的内容版本不可变）。对于静态内存或只读映射数据，读取器可直接通过 `mapped()` 暴露完整不可变切片（内置 `&'static [u8]` 读取器直接零拷贝借用底层切片并支持即时随机读取）。每次物理读取接管一个 `AssetBuffer` 并在 `ReadComplete` 中归还（即使读取失败也会归还缓冲）。

### 块缓存与会话容量预算 {#block-cache-and-session-budgets}

`ResourcePlugin` 注册具名字节提供方，`ImageResourcesPlugin::with_source` 将图像令牌映射到提供方键，并通过 `with_loader` 安装字节识别器与解码器。解码器识别 `LoadInput::Bytes`，通过 `FormatMatch::Need` 请求有界前缀，并使用 `LoadContext::poll_read_exact` 或 `poll_read` 读取数据。

内置 MIRX 字节解码器同时支持内存映射（mapped）与非映射随机访问的 `FLAT` 及 `CHUNK` 图像：映射 RAW 图像直接借用经校验的单平面像素并保留行跨距（stride）；非映射容器使用有界窗口校验目录、关键载荷与 `DATA` 校验和，编码图像则保留组元数据与单个可复用的单元输入缓冲。

### 按需视口像素驻留 (`poll_pixels_in`) {#lazy-viewport-pixel-residency}

加载完成后先发布图像几何尺寸与持有的 `AssetSource`，随后在真正需要绘制的视口区域内通过 `Texture::poll_pixels_in(region, context)` 按需读取 RAW 行或解码相交的编码块。像素尚未就绪时，`Image` 会先行绘制配置的回退占位图（自定义视图可使用 `paint.poll_draw(&command)` 在返回 `Poll::Pending` 时绘制替代内容），待像素就绪后自动在所属线程触发重绘。`ImageResourcesPlugin::with_decode_budget(bytes)` 限制内置源解码器的共享活动内存上限（默认按需准备 `128 KiB`）：

```rust,ignore
app.add_plugin(
    ImageResourcesPlugin::empty()
        .with_source("assets", |token| Some(format!("images/{token}.mirx").into()))
        .with_decode_budget(64 * 1024),
);

// 在异步消费者内部，使用当前任务上下文按需拉取局部区域像素：
let region = core::task::ready!(texture.poll_pixels_in(area, cx))?;
for (source_rect, pixels) in region.tiles() {
    consume_ready_pixels(source_rect, pixels);
}
```

也可以将 `ResourcePlugin`（配置底层 `AssetProvider` 与 `AssetCacheConfig` 块缓存）与 `ImageResourcesPlugin` 配合使用，或通过 `decoder.for_source(name, resolver)` 为无固定魔数头部的原始格式显式指定解码器：

```rust,ignore
app.add_plugin(
    ResourcePlugin::new()
        .with_source("assets", provider)
        .with_cache(AssetCacheConfig::new()
            .block_size(1024)
            .budget(MaxSize::Bytes(8 * 1024))
            .max_in_flight(2)),
);
app.add_plugin(
    ImageResourcesPlugin::empty()
        .with_source("assets", |token| {
            let id = token.strip_prefix("cover/")?;
            Some(format!("covers/{id}/thumbnail.r4").into())
        })
        .with_loader(|images| images.add_probed_loader(decoder)),
);
```

### 保留已打开的数据源 (`take_source`) {#retaining-an-opened-source}

在 `poll_load` 完成头部元数据读取后，解码器可调用 `LoadContext::take_source()` 将已打开的 `AssetSource` 所有权直接移入解码器状态或最终产出的资源中，无需重新打开键、拷贝源字节或分配新会话：

```rust,ignore
// 在 poll_load 内部，头部元数据读取完毕后转移数据源所有权：
state.source = Some(cx.take_source()?);

// 后续按需读取时，复用持有的 AssetSource 与当前任务的 waker：
let source = state.source.as_mut().unwrap();
let mut task = core::task::Context::from_waker(cx.waker());
let count = core::task::ready!(source.poll_read(offset, destination, &mut task))?;
```

### 块缓存与读取存储预算 (`AssetCacheConfig`) {#read-storage}

`AssetCacheConfig` 默认使用 `1 KiB` 块大小、`4 KiB` 共享载荷预算、最多 2 个在途物理读取、每个提供者 4 个预备会话、`1 KiB` 最大键长与 `256` 字节格式识别前缀（可通过 `max_requests`、`max_key_bytes` 与 `probe_bytes` 调整，绝不允许无界源缓存）。取消请求会立即停止解码与后续新读取，已提交的在途物理读取会在完成后安全归还缓冲区，而不会发布已取消的数据。
