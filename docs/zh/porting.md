# 屏幕、输入与硬件加速移植 {#porting-a-display-and-input-backend}

`mirui` 通过四组正交的接口将声明式 UI 树、布局引擎与响应式状态同底层硬件解耦：
- **`Surface`**：汇报屏幕逻辑尺寸与像素格式（`DisplayInfo`）、轮询原始输入事件（`InputEvent`），并将发生变化的物理像素脏矩形（`PhysicalRect`）刷上屏幕；
- **`FramebufferAccess` 与 `RendererFactory`**：将单缓冲或多缓冲 CPU 帧缓冲暴露给 `SwRendererFactory`（自动获得滚动像素平移复用与双缓冲脏区镜像同步），或绑定自定义 GPU 渲染器；
- **`RenderEngine` 与 `compose_backend!`**：拦截硬件外设支持的 2D 绘制操作（如 STM32 Chrom-ART DMA2D、NXP PXP、Vela VG-Lite），并将圆角裁剪、透视投影或复杂混合自动委托给软件光栅化器；
- **`set_clock`、`Storage` 与 `AudioSink`**：在 `no_std` 裸机或 RTOS 上接入硬件单调时钟、Flash 键值存储与 I2S/DMA 音频合成。

<MiruiSnippet recipe="porting-inspector" label="Surface & Dirty Window" file="recipes.rs" height="220px">

<template #exercise>

点击 `Touch Tap` 或 `Rotary +1` 切换触发重绘的 `PhysicalRect` 脏矩形，或点击 `RGB565 (2B)` / `RGBA8 (4B)` 切换像素格式，观察 `flush(area)` 如何计算 SPI 窗口指令（`CASET` / `RASET` 起止坐标）与本次 DMA 传输字节数。

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="从宿主循环与时钟经由 Surface 与 RenderEngine 到物理屏幕或 GPU 的四阶段后端移植边界">
  <svg viewBox="0 0 880 182" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="port-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="192" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · HOST &amp; CLOCK</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">宿主主循环与时钟</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">time::set_clock(ns_fn)</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">app.tick() / render_dirty()</text>
    <text x="32" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">驱动弹簧动画、定时器与输入</text>
    <path d="M208 91H246" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="254" y="22" width="198" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="270" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · SURFACE &amp; INPUT</text>
    <text x="270" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">显示表面与外设输入</text>
    <text x="270" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">display_info() · poll_event()</text>
    <text x="270" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">begin_flush → flush(area)</text>
    <text x="270" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">触摸屏、旋钮编码器与物理按键</text>
    <path d="M452 91H490" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="498" y="22" width="194" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="514" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · RENDER PIPELINE</text>
    <text x="514" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">软件、混合加速或 GPU</text>
    <text x="514" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">FramebufferAccess</text>
    <text x="514" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">compose_backend! (DMA2D)</text>
    <text x="514" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">滚动平移复用与双缓冲自动镜像</text>
    <path d="M692 91H730" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="738" y="22" width="126" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="754" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · HARDWARE</text>
    <text x="754" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">物理屏幕 / 扫描</text>
    <text x="754" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">SPI CASET/RASET</text>
    <text x="754" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">DRM / VSync 翻页</text>
    <text x="754" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">仅传输脏区像素</text>
  </svg>
</div>

## 目录 {#contents}

1. [选择移植层级](#choosing-a-porting-tier)
2. [Tier 1：用 `FramebufSurface` 快速点亮 SPI 屏幕](#tier-1-callback-framebuffer)
3. [Tier 2：实现 `Surface` 与 `FramebufferAccess`](#tier-2-custom-surface-and-framebuffer)
   - [双缓冲扫描输出与自动脏区镜像](#double-buffered-scanout-and-mirroring)
4. [接入触摸屏、旋钮编码器与物理按键](#wiring-touch-rotary-and-keys)
5. [Tier 3：用 `compose_backend!` 接入 2D 硬件加速](#tier-3-hardware-acceleration)
6. [裸机时钟、Flash 存储与 I2S 音频](#clocks-storage-and-audio)
7. [板级移植验收清单](#porting-checklist)

---

## 选择移植层级 {#choosing-a-porting-tier}

根据目标硬件的屏幕接口与内存形态，选择最轻量的接入层级：

| 层级 | 典型硬件场景 | 需要实现的接口 | 帧缓冲所有权 | 输入与硬件加速能力 |
| --- | --- | --- | --- | --- |
| **Tier 1 · 回调帧缓冲 (`FramebufSurface`)** | SPI / QSPI / I2C 小屏（`ST7789`、`ST7735`、`GC9A01`、`ILI9341`） | 无需实现任何 trait，只需传入一个 `\|buf, area: PhysicalRect\|` 闭包 | `FramebufSurface` 内部持有 `Vec<u8>`（`W × H × bpp`） | 适合纯显示或简单轮询；接入触摸芯片与 DMA 同步时可平滑升级至 Tier 2 |
| **Tier 2 · 自定义 `Surface` + `FramebufferAccess`** | 静态 BSS/SRAM DMA 缓冲、RTOS（`NuttX`、`Vela`、`Zephyr`、`FreeRTOS`）、Linux `fbdev` / `DRM` | 实现 `Surface` + `FramebufferAccess`（`framebuffer` 及可选的 `all_buffers`） | 调用方持有的静态数组、`mmap` 显存或双缓冲 DMA 环 | 完整的 `poll_event()` 队列（触控、旋钮、按键、滚轮）+ `SwRendererFactory` 自动双缓冲镜像 |
| **Tier 3 · 混合 2D 硬件加速或自定义 GPU** | 带 2D 加速外设的 MCU/SoC（`STM32 DMA2D`、`NXP PXP`、`VG-Lite`）或原生 GPU | 实现 `RenderEngine<SwRenderer>` + `compose_backend!` + `RendererFactory`（或完整 `Renderer`） | 与 `SwRenderer` 共享帧缓冲，或由设备管理 `BackendTarget` | 硬件加速矩形填充与纹理拷贝，圆角/路径/透视变换自动回退至软件光栅化 |

---

## Tier 1：用 `FramebufSurface` 快速点亮 SPI 屏幕 {#tier-1-callback-framebuffer}

如果板级工程已配置好全局分配器（`extern crate alloc`），并通过 SPI 或 8080 并口向外部屏幕控制器推送像素，直接使用内置的 `FramebufSurface` 即可在十几行代码内跑通画面。

### 像素格式与 SPI 字节序 (`RGB565Swapped` vs `RGB565`) {#pixel-formats-and-spi-byte-order}

绝大多数 SPI 屏幕驱动芯片（`ST7789`、`ST7735`、`GC9A01`、`ILI9341`）要求按 **高字节在前（MSB-first / 大端序）** 发送 16 位 RGB565 像素，而 ARM Cortex-M 与 RISC-V MCU 是小端序。此时应直接选择 `ColorFormat::RGB565Swapped`——软件光栅化器在画像素时就会直接写出高低字节交换后的 RGB565，SPI DMA 可以直接发送内存切片，无需在 `flush` 里再跑一遍字节翻转循环：

| `ColorFormat` | 每像素字节数 | 内存字节排列 | 适用屏幕与总线 |
| --- | --- | --- | --- |
| `RGB565Swapped` | `2` | `[msb, lsb]`（大端序 RGB565） | 小端序 MCU 通过 SPI / QSPI 驱动 `ST7789`、`GC9A01`、`ILI9341` 等屏幕 |
| `RGB565` | `2` | `[lsb, msb]`（原生小端序） | 并口 RGB / LTDC / MIPI DSI / Linux `fbdev` 16-bpp 显存 |
| `RGB888` | `3` | `[r, g, b]` 紧凑排列 | 24 位并口 RGB 或 MIPI DSI |
| `RGBA8888` | `4` | `[r, g, b, a]` | 桌面窗口、Web Canvas 与 GPU 纹理上传目标 |
| `BGRA8888` | `4` | `[b, g, r, a]` | Linux DRM、X11 与 Windows 原生扫描输出缓冲 |

### 按 `PhysicalRect` 切片发送 `CASET` / `RASET` 窗口指令 {#slicing-the-dirty-rectangle-for-caset-raset}

在调用 `app.render_dirty()`（或 `app.tick()`）期间，`FramebufSurface` 会针对本帧的每个物理脏矩形调用一次闭包。注意：`buf` 是完整的 `W × H` 全屏帧缓冲（行跨距 `STRIDE = W * bpp` 字节）；当局部脏区宽度小于全屏宽度（`area.width() < W`）时，请按行截取该矩形窗口对应的切片发送给 SPI：

```rust
use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::{RedrawStrategy, framebuf::FramebufSurface};
use mirui::types::PhysicalRect;

const W: u16 = 240;
const H: u16 = 240;
const BPP: usize = ColorFormat::RGB565Swapped.bytes_per_pixel(); // 2
const STRIDE: usize = W as usize * BPP;

fn push_spi_window(
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
    row_bytes: impl Iterator<Item = &'static [u8]>,
) {
    let _ = (x0, y0, x1, y1, row_bytes);
    // 1. 发送 CASET (0x2A)，参数为闭区间 [x0 >> 8, x0 as u8, x1 >> 8, x1 as u8]
    // 2. 发送 RASET (0x2B)，参数为闭区间 [y0 >> 8, y0 as u8, y1 >> 8, y1 as u8]
    // 3. 发送 RAMWR (0x2C)，随后通过 SPI DMA 依次发送每一行像素切片
}

let surface = FramebufSurface::with_format(
    W,
    H,
    ColorFormat::RGB565Swapped,
    |buf: &[u8], area: PhysicalRect| {
        if area.is_empty() {
            return;
        }
        let x0 = area.x();
        let y0 = area.y();
        let w = area.width();
        let h = area.height();
        let x1 = x0 + w - 1;
        let y1 = y0 + h - 1;

        // 若脏区横跨整屏宽度，可直接发起单次连续 DMA 传输：
        if x0 == 0 && w == W {
            let start = y0 as usize * STRIDE;
            let end = (y0 + h) as usize * STRIDE;
            let _contiguous = &buf[start..end];
        } else {
            // 否则按行遍历局部窗口内的像素切片：
            for row in y0..(y0 + h) {
                let offset = row as usize * STRIDE + x0 as usize * BPP;
                let _slice = &buf[offset..offset + w as usize * BPP];
            }
        }
        let _ = (x1, y1);
    },
)
// 可选：当 SPI 片选/指令建立开销较大时，可将多块稀疏脏区合并为单个外接矩形一次性刷新：
.with_redraw_strategy(RedrawStrategy::BoundingBox);

let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();
```

---

## Tier 2：实现 `Surface` 与 `FramebufferAccess` {#tier-2-custom-surface-and-framebuffer}

当你的硬件工程需要：
1. 将帧缓冲放在**静态 BSS / SRAM / PSRAM 段**（不经过堆分配）；
2. 在 `poll_event()` 中投递电容触摸、旋钮编码器或矩阵按键事件；
3. 利用 `begin_flush()` → `flush(area)` → `end_flush()` 协调异步 DMA 通道或 VSync 同步信号；或
4. 在**双缓冲 / 多缓冲**之间轮换扫描输出，

请直接在板级结构体上实现 `Surface` 与 `FramebufferAccess`。由于 `SwRendererFactory` 已经为所有实现了 `FramebufferAccess` 的类型提供了泛型实现（blanket impl），写好后直接调用 `App::new(your_surface)` 即可获得完整的软件光栅化与局部脏区更新能力：

```rust
use mirui::core::cache::InspectCaches;
use mirui::prelude::*;
use mirui::render::RenderError;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::surface::{
    BackbufferPersistence, DisplayInfo, FramebufferAccess, InputEvent, RedrawStrategy,
    SafeAreaInsets, Surface,
};
use mirui::types::{Fixed, PhysicalRect};

const PHYS_W: u16 = 240;
const PHYS_H: u16 = 280;
const FB_LEN: usize = PHYS_W as usize * PHYS_H as usize * 2;

pub struct BoardSurface {
    fb: [u8; FB_LEN],
    pending_events: [Option<InputEvent>; 8],
    event_head: usize,
    event_len: usize,
    dirty_count: u32,
}

impl BoardSurface {
    pub const fn new() -> Self {
        Self {
            fb: [0; FB_LEN],
            pending_events: [const { None }; 8],
            event_head: 0,
            event_len: 0,
            dirty_count: 0,
        }
    }

    pub fn push_input(&mut self, event: InputEvent) {
        if self.event_len < self.pending_events.len() {
            let slot = (self.event_head + self.event_len) % self.pending_events.len();
            self.pending_events[slot] = Some(event);
            self.event_len += 1;
        }
    }
}

impl InspectCaches for BoardSurface {}

impl Surface for BoardSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: PHYS_W,
            height: PHYS_H,
            scale: Fixed::ONE,
            format: ColorFormat::RGB565Swapped,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (u32::from(PHYS_W), u32::from(PHYS_H))
    }

    fn safe_area_insets(&self) -> SafeAreaInsets {
        // 为圆角表盘或传感器开孔预留逻辑安全区内边距：
        SafeAreaInsets {
            top: Fixed::from_int(12),
            right: Fixed::ZERO,
            bottom: Fixed::from_int(12),
            left: Fixed::ZERO,
        }
    }

    fn begin_flush(&mut self) -> Result<(), RenderError> {
        // 在本帧开始传输前等待上一轮异步 SPI DMA 完成。
        Ok(())
    }

    fn flush(&mut self, area: PhysicalRect) -> Result<(), RenderError> {
        if !area.is_empty() {
            self.dirty_count += 1;
            // 将 `self.fb` 中的 `area` 物理区域发送给屏幕控制器。
        }
        Ok(())
    }

    fn end_flush(&mut self) -> Result<(), RenderError> {
        // 本帧所有脏矩形传输完毕后提交（例如拉高 CS 或触发面板刷新）。
        Ok(())
    }

    fn abort_flush(&mut self) {
        // 若渲染中途报错中止，释放总线占用；未完成的脏区会由 App 自动保留到下一帧重试。
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if self.event_len == 0 {
            return None;
        }
        let event = self.pending_events[self.event_head].take();
        self.event_head = (self.event_head + 1) % self.pending_events.len();
        self.event_len -= 1;
        event
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Persistent
    }

    fn redraw_strategy(&self) -> RedrawStrategy {
        RedrawStrategy::Adaptive
    }
}

impl FramebufferAccess for BoardSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(
            &mut self.fb,
            PHYS_W,
            PHYS_H,
            ColorFormat::RGB565Swapped,
        )
        .expect("static buffer matches PHYS_W * PHYS_H * 2")
    }
}

let mut app = App::new(BoardSurface::new());
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();
```

### 双缓冲扫描输出与自动脏区镜像 {#double-buffered-scanout-and-mirroring}

在使用 LTDC / MIPI DSI / Linux DRM 等直接从内存扫描输出（scanout）的显示控制器时，单缓冲边画边扫会产生画面撕裂。使用双缓冲（一个前台缓冲供硬件扫描、一个后台缓冲供 `mirui` 绘制）时，局部脏区重绘要求两个缓冲区始终保持像素一致。

你**不需要**自己手写脏区拷贝逻辑，也不需要退化回每帧全屏重绘。只需让 `Surface::buffer_count` 返回 `2`，并在 `end_flush()` 翻转缓冲区后，通过 `FramebufferAccess::all_buffers` 返回 `[刚刚绘制完成的缓冲, 待同步的空闲缓冲]`。`SwRendererFactory::mirror_buffers` 会在 `end_flush()` 成功后自动完成两步同步：
1. 先在空闲缓冲上原地执行本帧的滚动像素平移（`plan.shifts`）；
2. 再将本帧实际重绘的物理脏矩形（`redraw`）从已完成缓冲拷贝到空闲缓冲中。

```rust
use alloc::{vec, vec::Vec};
use mirui::core::cache::InspectCaches;
use mirui::render::RenderError;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::surface::{DisplayInfo, FramebufferAccess, InputEvent, Surface};
use mirui::types::{Fixed, PhysicalRect};

const W: u16 = 320;
const H: u16 = 240;
const LEN: usize = W as usize * H as usize * 2;

pub struct DoubleBufferedSurface {
    buffers: [[u8; LEN]; 2],
    draw_index: usize,
    last_presented: usize,
}

impl InspectCaches for DoubleBufferedSurface {}

impl Surface for DoubleBufferedSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: W,
            height: H,
            scale: Fixed::ONE,
            format: ColorFormat::RGB565,
        }
    }

    fn flush(&mut self, _area: PhysicalRect) -> Result<(), RenderError> {
        Ok(())
    }

    fn end_flush(&mut self) -> Result<(), RenderError> {
        // 在 VSync 边界将扫描输出切换到 `self.draw_index`，并交换下一帧的可写槽位：
        self.last_presented = self.draw_index;
        self.draw_index ^= 1;
        Ok(())
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }

    fn buffer_count(&self) -> usize {
        2
    }
}

impl FramebufferAccess for DoubleBufferedSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(
            &mut self.buffers[self.draw_index],
            W,
            H,
            ColorFormat::RGB565,
        )
        .unwrap()
    }

    fn all_buffers(&mut self) -> Vec<Texture<'_>> {
        // 在 `end_flush()` 成功后调用：
        // 第一个元素必须是刚刚完成的帧缓冲（`last_presented`），后面依次是可写的非活动缓冲。
        let (first, second) = self.buffers.split_at_mut(1);
        let (completed, inactive) = if self.last_presented == 0 {
            (&mut first[0], &mut second[0])
        } else {
            (&mut second[0], &mut first[0])
        };
        vec![
            Texture::new(completed, W, H, ColorFormat::RGB565).unwrap(),
            Texture::new(inactive, W, H, ColorFormat::RGB565).unwrap(),
        ]
    }
}
```

---

## 接入触摸屏、旋钮编码器与物理按键 {#wiring-touch-rotary-and-keys}

`Surface::poll_event(&mut self)` 每次返回一条 `InputEvent`，直到队列排空返回 `None`。所有空间坐标（`PointerDown`、`PointerMove`、`PointerUp`、`Wheel`）均使用**逻辑像素坐标**（`0..display_info().width`、`0..display_info().height`）并以定点数 `Fixed` 表示：
- 当 `display_info().scale` 为 `Fixed::ONE`（`1.0`）时，直接用 `Fixed::from_int(raw_x)` 转换触摸芯片读出的整数坐标；
- 当启用了 HiDPI 缩放（例如 `480×480` 物理屏幕设置 `scale = 2.0`、逻辑视口 `240×240`）时，将物理触摸坐标除以缩放比：`Fixed::from_int(raw_x) / scale`。

```rust
use mirui::input::event::input::{
    InputEvent, KEY_HW_BUTTON_0, KEY_RETURN, KEY_ROTARY_PRESS, Key, PointerKind, Rotary,
};
use mirui::types::Fixed;

/// 电容触摸采样转换（如 FT6336 / CST816 / GT911）。
pub fn map_touch_sample(touching: bool, was_touching: bool, x: i32, y: i32) -> Option<InputEvent> {
    let x = Fixed::from_int(x);
    let y = Fixed::from_int(y);
    match (was_touching, touching) {
        (false, true) => Some(InputEvent::PointerDown {
            kind: PointerKind::Touch,
            id: 0,
            x,
            y,
        }),
        (true, true) => Some(InputEvent::PointerMove {
            kind: PointerKind::Touch,
            id: 0,
            x,
            y,
        }),
        (true, false) => Some(InputEvent::PointerUp {
            kind: PointerKind::Touch,
            id: 0,
            x,
            y,
        }),
        (false, false) => None,
    }
}

/// 正交旋钮编码器或智能手表表冠刻度（顺时针 +1，逆时针 -1）。
pub fn map_rotary_detent(delta: i16) -> InputEvent {
    Rotary { id: 0, delta }.into()
}

/// 表冠下按或侧边 GPIO 物理按键（`0..N`）。
pub fn map_hw_button(button_index: u32, pressed: bool) -> InputEvent {
    let code = if button_index == 0 {
        KEY_ROTARY_PRESS
    } else {
        KEY_HW_BUTTON_0 + (button_index - 1)
    };
    Key {
        code,
        physical: None,
        pressed,
        modifiers: None,
        repeat: Some(false),
    }
    .into()
}

/// 键盘/按键导航的确认键（触发聚焦控件的提交或默认动作）。
pub fn map_enter_key(pressed: bool) -> InputEvent {
    Key {
        code: KEY_RETURN,
        physical: None,
        pressed,
        modifiers: None,
        repeat: Some(false),
    }
    .into()
}
```

### 休眠生命周期与内存告警钩子 {#lifecycle-and-memory-pressure-hooks}

- **息屏挂起与唤醒恢复（`AppSuspend` / `AppResume`）**：在关闭屏幕背光或进入低功耗睡眠前投递 `InputEvent::AppSuspend`，唤醒后投递 `InputEvent::AppResume`。`App` 会在挂起时自动取消未完成的触摸拖拽手势，并触发 `PersistencePlugin` 将待存状态写入 Flash。
- **内存吃紧清理（`app.memory_warning()`）**：当系统堆水位告警时，调用 `app.memory_warning()` 可立即释放可重建的文本整形、路径排布与离屏渲染缓存，同时完整保留应用业务状态。

---

## Tier 3：用 `compose_backend!` 接入 2D 硬件加速 {#tier-3-hardware-acceleration}

许多嵌入式 SoC 内置了专用的 2D DMA 图形加速外设（例如 **STM32 Chrom-ART DMA2D**、**NXP PXP**、**Renesas DRW2D** 或 **Vela VG-Lite**）。这类外设通常能以总线满速填充无旋转矩形和搬运纹理，但不支持三次贝塞尔曲线填充、SDF 字形或 3D 透视投影四边形。

借助 `RenderEngine<T>` 与 `compose_backend!`，你**只需编写硬件外设擅长的那几个加速函数**，无需从零实现整套矢量与文字光栅化器：
1. 在 `route(&self, target, request)` 中检查 `DrawRequest`；
2. 若几何变换、圆角与不透明度完全符合硬件外设能力，返回 `Ok(RenderRoute::Native)` 并在 `submit` 中启动硬件加速；
3. 其余不支持的绘制请求直接委托给 `target.route(request)`——`compose_backend!` 会在从硬件引擎切回 `SwRenderer` 时自动调用一次 `end(&mut self, target)`（等待 DMA 完成屏障），随后由 `SwRenderer` 在同一块帧缓冲上无缝接力绘制。

```rust
use mirui::core::cache::InspectCaches;
use mirui::render::{
    DrawCommand, DrawRequest, PreparedDraw, RenderEngine, RenderError, RenderRoute, Renderer,
    RendererFactory, SwRenderer, SwRendererFactory,
};
use mirui::surface::FramebufferAccess;
use mirui::types::{Fixed, Viewport};
use mirui_macros::compose_backend;

#[derive(Default)]
pub struct Dma2dEngine {
    pub hw_fills: u32,
}

impl<'a> RenderEngine<SwRenderer<'a>> for Dma2dEngine {
    fn route(
        &self,
        target: &SwRenderer<'a>,
        request: &DrawRequest<'_, '_>,
    ) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        // 仅用硬件加速无旋转、无透视、直角（radius == 0）且不透明的纯色矩形填充：
        if request.projective.is_identity()
            && request.command.transform().is_identity()
            && let DrawCommand::Fill {
                quad: None,
                radius,
                opa: 255,
                ..
            } = request.command
            && *radius == Fixed::ZERO
        {
            return Ok(RenderRoute::Native);
        }
        // 圆角矩形、矢量路径、字形与复杂混合自动交还给 SwRenderer：
        target.route(request)
    }

    fn begin(&mut self, _target: &mut SwRenderer<'a>) -> Result<(), RenderError> {
        // 进入连续硬件绘制会话前配置外设寄存器。
        Ok(())
    }

    fn submit(
        &mut self,
        target: &mut SwRenderer<'a>,
        draw: &PreparedDraw<'_>,
    ) -> Result<(), RenderError> {
        let request = draw.request();
        if self.route(target, &request)? == RenderRoute::Native
            && let DrawCommand::Fill { area, color, .. } = request.command
        {
            self.hw_fills += 1;
            // 实际驱动中在此处根据 `target.viewport.physical_rect(...)` 配置 DMA2D 寄存器并启动填充；
            // 此处转发给软件目标以保证示例可直接独立运行：
            let _ = (area, color);
        }
        target.submit(draw)
    }

    fn end(&mut self, _target: &mut SwRenderer<'a>) {
        // 在切回 CPU 软件光栅化器访问同一块 RAM 帧缓冲前，等待 DMA2D 传输完成中断标志。
    }
}

compose_backend! {
    pub struct Dma2dHybrid<'a> {
        sw: SwRenderer<'a>,
        dma2d: Dma2dEngine,
    }
    route {
        default => sw,
        fill_rect => dma2d,
    }
}

#[derive(Default)]
pub struct Dma2dFactory {
    sw: SwRendererFactory,
}

impl InspectCaches for Dma2dFactory {}

impl<B: FramebufferAccess> RendererFactory<B> for Dma2dFactory {
    type Renderer<'a>
        = Dma2dHybrid<'a, SwRenderer<'a>, Dma2dEngine>
    where
        Self: 'a,
        B: 'a;

    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a> {
        let sw = self.sw.make(backend, transform);
        Dma2dHybrid::new(sw, Dma2dEngine::default())
    }

    fn mirror_buffers(
        &mut self,
        backend: &mut B,
        plan: &mirui::ui::dirty::DirtyRegions,
        redraw: &[mirui::types::PhysicalRect],
        transform: &Viewport,
    ) -> Result<(), RenderError> {
        self.sw.mirror_buffers(backend, plan, redraw, transform)
    }

    fn trim_memory(&mut self) {
        self.sw.trim_memory();
    }
}
```

构造应用时调用 `App::with_factory(surface, Dma2dFactory::default())` 即可启用混合加速管线。如果目标平台是拥有独立显存纹理的完整 GPU 后端，请直接实现 `Renderer` 与 `BackendTarget`（详见[各平台渲染目标存储](backend-targets.md)与[混合后端与软件回退](backend-fallback.md)）。

---

## 裸机时钟、Flash 存储与 I2S 音频 {#clocks-storage-and-audio}

### 1. 单调时钟注入 (`mirui::core::time::set_clock`) {#_1-monotonic-clock-mirui-core-time-set-clock}

在 `std` 环境下，`mirui` 会在首次读取时自动锚定系统单调时钟；而在 `no_std` 裸机或 RTOS 环境下，请在板级初始化时调用 `mirui::core::time::set_clock` 注册纳秒级时钟函数，并通过插件在每帧更新 `MonoClock` 资源，从而驱动弹簧动画、补间、长按手势判定与 `#[timer]` 定时器：

```rust
use mirui::app::{App, Plugin};
use mirui::core::time;
use mirui::ecs::MonoClock;

fn board_uptime_ns() -> u64 {
    // 读取硬件定时器、DWT 周期计数器或 RTOS tick 并折算为纳秒：
    16_666_667
}

pub struct BoardClockPlugin;

impl Plugin for BoardClockPlugin {
    fn build<B: mirui::surface::Surface, F: mirui::render::RendererFactory<B>>(
        &self,
        app: &mut App<B, F>,
    ) {
        time::set_clock(board_uptime_ns);
        app.world.insert_resource(MonoClock::new(0));
    }

    fn on_tick<B: mirui::surface::Surface, F: mirui::render::RendererFactory<B>>(
        &mut self,
        app: &mut App<B, F>,
    ) {
        let now_ms = time::clock_now_ns() / 1_000_000;
        if let Some(mut clock) = app.world.get_resource_mut::<MonoClock>() {
            clock.advance_to(now_ms);
        }
    }
}
```

### 2. Flash 键值存储对接 (`mirui::core::storage::Storage`) {#_2-flash-kv-storage-mirui-core-storage-storage}

若要将 `#[model(persistence)]` 状态保存到 SPI NorFlash、LittleFS、KVDB、NVS 或 EEPROM，只需实现同步的 `Storage` trait 并传给 `PersistencePlugin::new(storage)`。当待写入的字节与已存储字节相同时，返回 `WriteOutcome::Unchanged` 即可跳过擦写，保护 Flash 寿命：

```rust
use alloc::vec::Vec;
use mirui::core::storage::{Storage, StorageError, WriteOutcome};

pub struct FlashKvStorage;

impl Storage for FlashKvStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let _ = key;
        // 仅当键不存在时返回 Ok(None)；遇到 CRC 校验失败或总线错误时返回 Err(StorageError::Io)。
        Ok(None)
    }

    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        if self.read(key)?.as_deref() == Some(value) {
            return Ok(WriteOutcome::Unchanged);
        }
        // 将 `value` 字节写入对应 `key` 的 Flash / KV 扇区：
        Ok(WriteOutcome::Written)
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let _ = key;
        Ok(())
    }
}
```

### 3. I2S / DMA 音频合成 (`AudioMixer` 与 `AudioSink`) {#_3-i2s-dma-audio-synthesis-audiomixer-audiosink}

启用 `audio` 特性后，`AudioMixer<V>` 可以在**零堆分配**的前提下将多声部合成波形与乐谱直接渲染进调用方提供的 `i16` PCM 缓冲区。在 I2S DMA 半传输/全传输中断或 RTOS 音频线程中调用 `mixer.render_mono(&mut dma_half_buffer)`，并实现 `AudioSink` 接收来自 `app.audio()` 的控制命令：

```rust
use mirui::audio::{AudioBank, AudioCommand, AudioMixer, AudioSink, MixerError};

static BANK: AudioBank = AudioBank::new(&[]);

pub struct I2sAudioSink {
    mixer: AudioMixer<8>,
}

impl I2sAudioSink {
    pub fn new(sample_rate: u32) -> Result<Self, MixerError> {
        Ok(Self {
            mixer: AudioMixer::new(&BANK, sample_rate)?,
        })
    }

    /// 在 I2S DMA 半缓冲中断或 RTOS 音频线程中调用：
    pub fn fill_dma_buffer(&mut self, pcm_mono: &mut [i16]) {
        self.mixer.render_mono(pcm_mono);
    }
}

impl AudioSink for I2sAudioSink {
    type Error = MixerError;

    fn start(&mut self, _bank: &'static AudioBank) -> Result<(), Self::Error> {
        Ok(())
    }

    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error> {
        self.mixer.apply(command)
    }
}
```

---

## 板级移植验收清单 {#porting-checklist}

1. **无头冒烟测试（Headless Smoke Test）**：在烧录开发板之前，先在宿主机 `cargo test` 中把应用的 `#[compose]` 树挂载到 `App::headless(W, H)` 上并调用 `app.render().unwrap()`，提前验证布局尺寸与字体/路径容量预留是否充足。
2. **三色条与字节序校验**：在屏幕上并排画红（`Color::rgb(255, 0, 0)`）、绿（`Color::rgb(0, 255, 0)`）、蓝（`Color::rgb(0, 0, 255)`）三条色块。如果红蓝颠倒，切换屏幕初始化的 `MADCTL` BGR 位（或在 `RGBA8888` / `BGRA8888` 间切换）；如果绿色条出现高低位断裂杂色，在 `RGB565` 与 `RGB565Swapped` 之间切换。
3. **局部脏矩形跨距校验**：点击一个独立小按钮触发局部刷新（此时 `area.width() < W`）。如果局部刷新区域出现斜纹或错位，检查 `CASET` / `RASET` 的结束坐标是否写成了闭区间 `(x + w - 1, y + h - 1)`，以及逐行切片时源缓冲区偏移是否按整屏跨距 `W * bpp` 递增。
4. **触控与旋钮坐标对齐**：在屏幕四角各放一枚 `Button`，点击验证 `PointerDown` / `PointerUp` 的 `(x, y)` 轴向是否与屏幕旋转方向（`MADCTL` 的 `MV` / `MX` / `MY` 位）一致。
5. **时钟与空闲休眠验证**：确认界面静止且无动画时 `app.tick()` 返回 `false`，此时主循环应进入低功耗等待（`wfi` 或带下一次定时器超时时间的 RTOS 信号量等待），直到收到下一条外部触控中断或定时唤醒。
