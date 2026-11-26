# 静态与托管资源池 {#resources}

`ResourceManager<T>` 在既有缓存预算下将资源令牌（token）解析为共享值。`ResourceHandle::get()` 与 `ResourceManager::resolve()` 返回已完成值或管理器回退值（fallback）的 `cache::Handle<T>`，直接共享同一缓存条目而不拷贝底层内容。已获取的值句柄在条目被替换、驱逐乃至管理器被销毁后依然可读；而通过需求句柄（`ResourceHandle`）发起的下一次读取则会自动跟随管理器的当前最新值。静态注册项与已完成句柄的读取不会触发加载器（loader）。

<div class="arch-diagram" role="img" aria-label="从 ImageSource 与 ResourceManager 经由 ProbeLoader 与 AssetProvider 到受预算管控的像素驻留流水线">
  <svg viewBox="0 0 880 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="res-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="194" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · SOURCE BINDING</text>
    <text x="32" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">令牌、路径或纹理</text>
    <text x="32" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Image(src: ...) · prop!</text>
    <text x="32" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ImageSource::from</text>
    <text x="32" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">需求句柄与响应式订阅</text>
    <path d="M210 84H248" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#res-arrow)" />
    <rect x="256" y="20" width="194" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="272" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · MANAGER &amp; PROBE</text>
    <text x="272" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">缓存与协作式轮询</text>
    <text x="272" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">ResourceManager&lt;T&gt;</text>
    <text x="272" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Loader · ProbeLoader</text>
    <text x="272" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">每个加载器 4 个并发调用槽位</text>
    <path d="M450 84H488" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#res-arrow)" />
    <rect x="496" y="20" width="186" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="512" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · BYTE PROVIDER</text>
    <text x="512" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">内存映射或分块 I/O</text>
    <text x="512" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">AssetProvider · AssetSource</text>
    <text x="512" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">AssetCacheConfig</text>
    <text x="512" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">有界块缓存与在途 I/O 控制</text>
    <path d="M682 84H720" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#res-arrow)" />
    <rect x="728" y="20" width="136" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="744" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · RESIDENCY</text>
    <text x="744" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">像素解码与驻留</text>
    <text x="744" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">cache::Handle&lt;T&gt;</text>
    <text x="744" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">poll_pixels_in</text>
    <text x="744" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">解码预算与优雅降级</text>
  </svg>
</div>

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

`AssetProvider` 按键打开对象，其关联的 `AssetReader` 汇报长度、定位与内容一致性（`SourceConsistency::Stable` 保证跨多次读取的内容版本不可变）。对于静态内存数据，内置的 `&'static [u8]` 读取器直接通过 `mapped()` 暴露零拷贝切片；对于外部闪存或文件系统，则通过 `ResourcePlugin` 与 `ImageResourcesPlugin` 绑定分块异步读取与格式解码器。

加载完成后先发布图像几何尺寸与持有的 `AssetSource`，随后在真正需要绘制的视口区域内通过 `Texture::poll_pixels_in(region, context)` 按需读取 RAW 行或解码相交的编码块。像素尚未就绪时，`Image` 会先行绘制配置的回退占位图，待像素就绪后自动在所属线程触发重绘。`ImageResourcesPlugin::with_decode_budget(bytes)` 限制内置源解码器的共享活动内存上限（默认按需准备 `128 KiB`）：

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
