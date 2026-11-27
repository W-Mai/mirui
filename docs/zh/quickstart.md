# 环境搭建与多端工程 {#quickstart}

本文介绍如何从零开始在桌面端、移动端、WebAssembly 浏览器或嵌入式 MCU 硬件上跑起一个共享同一套 UI 模型的 `mirui` 应用。

阅读本文只需熟悉基本的 `cargo` 与 Rust 语法；如果你打算直接点亮一块嵌入式 SPI 屏幕，还需要一点翻阅屏幕驱动 IC 数据手册的习惯。

<div class="arch-diagram" role="img" aria-label="共享同一个 no_std UI 核心库并同时驱动桌面端、Web 浏览器、移动端与嵌入式 MCU 的多端 Cargo 工作区架构">
  <svg viewBox="0 0 880 188" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="qs-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="24" width="236" height="140" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="50" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">SHARED CORE · app/</text>
    <text x="32" y="74" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Shared UI Crate (no_std + alloc)</text>
    <text x="32" y="98" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">#[model] · #[compose] · ui!</text>
    <text x="32" y="118" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">#[view] · #[system] · StyleSheet</text>
    <text x="32" y="142" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">100% 平台无关的共享 UI 逻辑</text>
    <path d="M252 54H304" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#qs-arrow)" />
    <path d="M252 94H304" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#qs-arrow)" />
    <path d="M252 134H304" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#qs-arrow)" />
    <rect x="312" y="20" width="268" height="68" rx="10" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="328" y="42" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">TARGET 01 · DESKTOP</text>
    <text x="328" y="61" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13" font-weight="620">Desktop SDL / WGPU</text>
    <text x="328" y="78" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11">SdlSurface · SdlGpuSurface · WgpuSurface</text>
    <rect x="596" y="20" width="268" height="68" rx="10" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="612" y="42" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">TARGET 02 · BROWSER</text>
    <text x="612" y="61" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13" font-weight="620">Web Canvas (Trunk Wasm)</text>
    <text x="612" y="78" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11">WebCanvasSurface · wasm32-unknown-unknown</text>
    <rect x="312" y="100" width="268" height="68" rx="10" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="328" y="122" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">TARGET 03 · EMBEDDED</text>
    <text x="328" y="141" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13" font-weight="620">Embedded MCU (ESP32 / NuttX / STM32)</text>
    <text x="328" y="158" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11">FramebufSurface · DMA2D · RGB565 SPI</text>
    <rect x="596" y="100" width="268" height="68" rx="10" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="612" y="122" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">TARGET 04 · MOBILE</text>
    <text x="612" y="141" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13" font-weight="620">Mobile (Android / iOS)</text>
    <text x="612" y="158" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11">MobileHost · NativeActivity · UIWindowScene</text>
  </svg>
</div>

## 工具链准备 {#toolchain-prerequisites}

桌面端开发只需安装稳定版 Rust（`1.85` 及以上）：

```bash
rustup default stable
rustup update
```

如果要编译并烧录到 ESP32-C3 开发板，还需要添加 `riscv32imc` 目标三元组以及 `espflash` 烧录工具（两者同样直接使用稳定版 Rust）：

```bash
rustup target add riscv32imc-unknown-none-elf
cargo install espflash
```

工作区模板中的桌面端与 ESP32-C3 目标均使用 stable 工具链；浏览器（Wasm）与移动端目标则按需安装各自的平台打包工具。

## 桌面端 SDL {#desktop-sdl}

SDL 后端动态链接系统中的 SDL2 库，请先通过系统包管理器安装运行库与头文件：

```bash
brew install sdl2-compat sdl3 # macOS
apt-get install libsdl2-dev   # Debian / Ubuntu
```

macOS 桌面端构建要求 macOS 11 及以上版本。建议安装 `sdl2-compat` 并配合 `SDL 3.4.14` 及以上版本，以获得高分辨率的触控板平滑滚动与原生惯性动量（原版 SDL2 运行库不支持该特性）。Rust 侧 `sdl2` crate 的版本号与底层原生动态库版本相互独立；升级系统动态库后重启应用即可生效。

`sdl` 与 `sdl-gpu` 两个后端默认均开启渲染器命令批处理（可通过环境变量 `SDL_RENDER_BATCHING` 或 SDL hint 覆盖）。由于帧回读（readback）会强制同步排队中的渲染命令，测量日常帧耗时时请排除截图操作。

`sdl-gpu` 后端支持显式配置图形驱动、多重采样（MSAA）、垂直同步与窗口模式：

```rust,no_run
use mirui::surface::sdl_gpu::{SdlGpuConfig, SdlGpuDriver, SdlGpuSurface, SdlWindowMode};

let surface = SdlGpuSurface::with_config("Demo", 640, 480, SdlGpuConfig {
    driver: SdlGpuDriver::OpenGl { samples: 4 },
    window: SdlWindowMode::Windowed { resizable: true },
    vsync: true,
})?;
println!("Renderer: {}", surface.renderer_name());
# Ok::<(), mirui::surface::sdl_gpu::SdlGpuError>(())
```

`SdlGpuSurface::new(title, width, height)` 默认使用窗口化 OpenGL、4× MSAA 与垂直同步。显式指定 `OpenGl` 或 `OpenGles2` 时不会静默降级到其他渲染器；`Auto` 则交由 SDL 自动选择硬件加速驱动（不主动请求 GL 多重采样）。OpenGL 下 `samples: 0` 关闭 MSAA，`1` 为非法值，大于 1 的值请求至少对应倍数的采样；若硬件不支持所请求的配置，构造器会直接返回错误，而不会静默降低画质。

支持渲染目标（render target）的单重采样窗口会保留其 GPU 像素以支持局部脏区重绘；开启了多重采样的窗口则保留窗口级 MSAA 并在出现脏区时整帧重绘；两者在界面无变化时均保持零绘制开销。关于保留目标内存与复合回退规则，请参阅[混合后端与软件回退](backend-fallback.md#backend-behavior)。

`SdlGpuFactory::new().with_texture_cache_budget(bytes)` 与 `WgpuRendererFactory::new().with_texture_cache_budget(bytes)` 用于配置已上传纹理的驻留缓存上限（独立于字形缓存与目标区域编辑缓冲），默认均为按需分配的 `16 MiB`（设为 `0` 则禁用驻留缓存，超大纹理仍可直接绘制但不缓存）。

下面从零创建一个桌面工程：

```bash
cargo new hello-mirui
cd hello-mirui
```

编辑 `Cargo.toml`：

```toml
[package]
name = "hello-mirui"
version = "0.1.0"
edition = "2024"

[dependencies]
mirui = { version = "0.57", features = ["sdl"] }
```

编写 `src/main.rs`：

```rust
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::ui::widgets::{ParagraphStyle, Text};

#[compose]
fn build_ui() {
    ui! {
        Column (grow: 1.0) {
            View (
                bg_color: ColorToken::Primary,
                text_color: ColorToken::OnPrimary,
                height: 40,
                border_radius: 8,
                padding: Padding::all(10)
            ) {
                Text ("Hello mirui!")
            }
            View (bg_color: ColorToken::SurfaceVariant, grow: 1.0)
            Text ("ECS + DSL", height: 30, paragraph: ParagraphStyle::label())
        }
    };
}

fn main() {
    let backend = SdlSurface::new("hello mirui", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = app.spawn_root().id();
    app.compose(root, build_ui());

    app.run();
}
```

运行工程：

```bash
cargo run
```

此时会弹出一个 `480×320` 的桌面窗口，包含顶部的蓝色标题栏、中间自适应伸展的内容区以及底部的说明文字栏。

如果在 Apple Silicon 的 macOS 上运行 `cargo run` 时提示找不到 `libSDL2.dylib`，请将 Homebrew 库路径加入环境变量：

```bash
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
```

### 可选音频输出 (`AudioPlugin`) {#optional-audio}

在桌面端启用 `sdl-audio` 特性或在浏览器端启用 `web-audio` 特性后，可通过 `AudioPlugin` 注册定长容量的音频命令总线，并通过 `app.audio()` 获取 `AudioHandle`（裸机环境可使用 `AudioMixer` 填充调用方持有的 PCM 缓冲区，或为 I2S/DMA 驱动实现 `AudioSink`）：

```rust
use mirui::app::plugins::AudioPlugin;
use mirui::audio::{AudioBank, AudioTone, SdlAudioSink, Waveform};

static AUDIO: AudioBank = AudioBank::new(&[]);

app.add_plugin(AudioPlugin::new(SdlAudioSink::new(), &AUDIO));

if let Some(audio) = app.audio() {
    audio.tone(AudioTone::new(72, Waveform::Sine, 240, 190));
}
```

`audio.state()` 返回当前输出状态快照；`audio.state_signal()` 返回对应的只读响应式信号，可在 `ui!` 属性中调用 `.get()` 实时跟随静音与输出状态变化。

## ESP32-C3 嵌入式 {#esp32-c3-embedded}

嵌入式路线只需要一块开发板、一块 SPI 小屏幕和一根 USB 数据线。`mirui` 在 MCU 侧的核心接入代码非常精简——`no_std` 的 `main`、`esp-hal` 外设初始化，外加一个在刷新回调里把像素字节推向 SPI 屏幕的 `FramebufSurface`。由于各代 `esp-hal` 的时钟、DMA 描述符与 ST7735/ST7789/GC9A01 初始化序列会随上游版本演进，完整的板级工程维护在官方示例仓库 [mirui-examples](https://github.com/W-Mai/mirui-examples) 中：

```bash
git clone https://github.com/W-Mai/mirui-examples
cd mirui-examples/examples/esp32c3-animation
cargo build --release --no-default-features --features=demo-threebody
```

其中 `src/board.rs` 封装了 SPI + ST7735 屏幕初始化，`src/main.rs` 将其接入 `App::new(FramebufSurface::…)`。构建完成后使用以下命令一键烧录并打开串口监视器：

```bash
espflash flash --monitor target/riscv32imc-unknown-none-elf/release/mirui-esp32c3
```

在任何 MCU 开发板上，`mirui` 一侧的接入方式完全通用：

```rust
let backend = FramebufSurface::with_format(
    W, H,
    mirui::render::texture::ColorFormat::RGB565Swapped,  // 或 RGB565
    |bytes: &[u8], area: mirui::types::PhysicalRect| {
        // 将 `area` 脏区矩形对应的 `bytes` 通过 SPI 写入 LCD 窗口。
    },
);

let mut app = App::new(backend);
app.with_default_widgets().with_default_systems();
// 后续的 app.spawn_root()、构建 ui! 树与 app.run() 与桌面端完全一致。
```

`ColorFormat::RGB565Swapped` 是小端序 MCU 通过 SPI 驱动绝大多数 ST7735/ST7789 屏幕时所需的字节序；若屏幕接收标准顺序则改用 `RGB565`。关于 `display_info`、`flush`、`poll_event` 与像素持久性等完整契约，请参阅 [`Surface` trait 源码](../../src/surface/mod.rs)。

## Cargo 多端工作区 {#cargo-workspace}

当同一套界面既要在电脑窗口上快速预览调试，又要直接编译进 MCU 固件或浏览器画布时，推荐将界面代码放在一个兼容 `std` / `no_std` 的共享库 crate（`app/`）中，再为每个硬件平台建立薄薄的宿主二进制 crate（`targets/*`）：

```
my-app/
├── Cargo.toml             # [workspace] members = ["app", "targets/*"]
├── app/                   # 共享 UI 库，支持 std / no_std 双模式
│   ├── Cargo.toml
│   └── src/lib.rs
└── targets/               # 每个运行平台一个二进制 crate（通配符自动收录）
    ├── desktop/           # SDL 桌面窗口入口
    │   ├── Cargo.toml
    │   └── src/main.rs
    └── esp32c3/           # ESP32-C3 固件入口
        ├── Cargo.toml
        ├── .cargo/config.toml
        ├── rust-toolchain.toml
        └── src/main.rs
```

根目录 `Cargo.toml`：

```toml
[workspace]
resolver = "2"
members = ["app", "targets/*"]
default-members = ["app", "targets/desktop"]
```

共享 UI 库 `app/Cargo.toml`：

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[features]
default = []
std = ["mirui/std"]

[dependencies]
mirui = { version = "0.57", default-features = false, features = ["quad-aa"] }
```

共享 UI 库 `app/src/lib.rs`：

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use mirui::prelude::*;
use mirui::ui::widgets::Text;

#[compose]
pub fn build_ui() -> Entity {
    ui! {
        View (bg_color: ColorToken::Surface) {
            Text ("Hello mirui!", text_color: ColorToken::OnSurface)
        }
    }
}
```

`targets/desktop/Cargo.toml` 启用 `mirui/sdl` 与 `app/std`；`targets/esp32c3/Cargo.toml` 则保持 `default-features = false` 并接入 `esp-hal`。各目标的 `main.rs` 只需创建各自的 `Surface` 与 `App`，再调用 `app.compose(root, app::build_ui())` 即可。

### 新增硬件目标 {#adding-a-new-target}

若要继续扩展 ESP32-S3、RP2040 或 STM32 等新目标：

1. 复制一份现有目标目录：`cp -r targets/esp32c3 targets/esp32s3`；
2. 修改新目录 `Cargo.toml` 中的包名与板级 HAL 依赖；
3. 更新 `.cargo/config.toml` 与 `rust-toolchain.toml` 中的目标三元组与链接脚本；
4. 在 `src/main.rs` 中对接新芯片的时钟、SPI 与引脚，随后运行 `cargo build -p esp32s3` 即可（`targets/*` 通配符会自动将其纳入工作区）。

## WGPU 渲染配置 {#wgpu-configuration}

`WgpuSurface::with_config` 与 `MobileWgpuSurface::with_config` 接收统一的设备与呈现配置：

```rust
use mirui::surface::wgpu_surface::{WgpuConfig, WgpuSurface, PresentMode, PowerPreference};

let surface = WgpuSurface::with_config("Demo", 640, 480, WgpuConfig {
    present_mode: Some(PresentMode::Fifo),
    power_preference: PowerPreference::LowPower,
    maximum_frame_latency: 2,
    samples: 4,
})?;
```

默认呈现模式按 `Mailbox → Immediate → Fifo` 顺序优选；显式指定某一种模式时要求底层设备必须支持，而 `AutoVsync` 与 `AutoNoVsync` 则保留 WGPU 的自动回退策略。桌面端默认启用 4× MSAA，移动端默认为 1×（`samples: 1` 表示关闭 MSAA）。

## Android 与 iOS 移动端工程 {#android-and-ios}

`android` 与 `ios` 模板支持在生成时选择渲染路径：`wgpu` 直接向平台 WGPU 表面提交绘制命令；`sw` 则在内存中保留一块受预算约束的 RGBA 帧缓冲，使用软件后端光栅化脏区后仅通过 WGPU 呈现上屏。两条路径在应用切到后台挂起（suspend）并恢复（resume）时都会完整保留 `App`、ECS World 与响应式模型状态，仅重建底层平台表面：

```bash
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/android --name hello-mirui-android
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/ios --name hello-mirui-ios
```

Android 采用 `NativeActivity` 入口，通过 `cargo-ndk` 编译 Rust 并由内置 Gradle 包装器打包 APK；iOS（要求 iOS 14.0+）生成 Rust 静态库与配套的精简 Xcode 宿主工程。其 `Info.plist` 声明单个应用 Scene，由 `MobileHost` 自动接管窗口挂载、多点触控 ID 映射、原生软键盘唤醒以及系统内存告警时的缓存释放（`Plugin::on_memory_warning`）：

```xml
<key>UIApplicationSceneManifest</key>
<dict>
    <key>UIApplicationSupportsMultipleScenes</key>
    <false/>
    <key>UISceneConfigurations</key>
    <dict>
        <key>UIWindowSceneSessionRoleApplication</key>
        <array>
            <dict>
                <key>UISceneConfigurationName</key>
                <string>Default Configuration</string>
            </dict>
        </array>
    </dict>
</dict>
```

`app.spawn_root()` 默认将子节点约束在系统安全区（Safe Area）之内；需要全屏铺满背景时可调用 `app.spawn_root().ignore_safe_area().id()`。

## 使用 `cargo-generate` 一键生成脚手架 {#skip-the-boilerplate}

安装 [`cargo-generate`](https://github.com/cargo-generate/cargo-generate) 后，可直接从官方模板仓库一键生成任意平台的工程脚手架：

```bash
cargo install cargo-generate
```

```bash
# 桌面端工程（按提示选择 WGPU 或 SDL）
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/desktop --name hello-mirui

# 单目标 ESP32-C3 固件工程
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/esp32c3 --name hello-mirui-esp32c3

# 多端共享 Cargo 工作区（含共享 app 库 + desktop / wasm / esp32c3 / android / ios 五端入口）
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/workspace --name my-app

# 浏览器 Canvas 2D (WebAssembly + Trunk)
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/wasm --name hello-mirui-web

# Android NativeActivity 工程
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/android --name hello-mirui-android

# iPhone / iPad Xcode 工程
cargo generate --git https://github.com/W-Mai/mirui-templates.git templates/ios --name hello-mirui-ios
```

## 下一步去哪 {#where-to-go-next}

- **写一个完整应用**：跟着[动手教程](learn/index.md)从空窗口写到带持久化与弹簧过渡的任务清单；
- **速查常见 UI 模式**：查看[常用界面写法](learn/recipes.md)里的状态换色、弹性布局、弹簧跟随框、自定义矢量控件、曲线排字与平滑滚动示例；
- **深入声明式状态与组合**：阅读[响应式状态与样式](state-management.md)与[组合、角色与自定义视图](composition.md)；
- **处理手势、按键与自定义控件**：阅读[手势识别与事件回调](events.md)、[自定义控件与绘制](widget-events.md)与[文本编辑与输入法](text-editing.md)；
- **移植新屏幕、触摸芯片或 2D 硬件加速**：阅读[屏幕、输入与硬件加速移植](porting.md)，按三层渐进式路径对接 `FramebufSurface`、双缓冲 `Surface`、触控/表冠事件与 `compose_backend!` DMA2D 加速；
- **翻阅完整可运行示例**：参考仓库内的 [`gallery/examples/`](../../gallery/examples/) 与 [`mirui-examples`](https://github.com/W-Mai/mirui-examples)。
