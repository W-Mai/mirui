# 屏幕、输入与硬件加速移植 {#porting-a-display-and-input-backend}

`mirui` 通过四组正交的接口将声明式 UI 树、布局引擎与响应式状态同底层硬件解耦：
- **`Surface`**：汇报屏幕逻辑尺寸与像素格式（`DisplayInfo`）、排空原始输入事件（`InputEvent`），并将发生变化的物理像素脏矩形（`PhysicalRect`）刷上屏幕；
- **`FramebufferAccess` 与 `RendererFactory`**：将单缓冲或多缓冲 CPU 帧缓冲暴露给 `SwRendererFactory`（自动获得滚动像素平移复用与双缓冲脏区镜像同步），或绑定自定义 GPU 渲染器；
- **`RenderEngine` 与 `compose_backend!`**：将受支持的操作路由给 2D 位块搬运外设（如 `STM32 DMA2D`、`NXP PXP`）或 2.5D 矢量 GPU（如支持硬件路径绘制与 `vg_lite_query_feature` 动态特性查询的 `VeriSilicon VG-Lite`），仅在遇到当前硬件不支持的特性时回退给软件光栅化器；
- **`set_clock`、`Storage` 与 `AudioSink`**：在 `no_std` 裸机或 RTOS 上接入硬件单调时钟、Flash 键值存储与 I2S/DMA 音频合成。

<MiruiSnippet recipe="porting-inspector" label="Surface & Dirty Window" file="recipes.rs" height="220px">

<template #exercise>

点击 `Touch Tap` 或 `Rotary +1` 切换触发重绘的 `PhysicalRect` 脏矩形，或点击 `RGB565 (2B)` / `RGBA8 (4B)` 切换像素格式，观察 `flush(area)` 如何计算 SPI 窗口指令（`CASET` / `RASET` 起止坐标）与本次 DMA 传输字节数。

</template>

</MiruiSnippet>

<ArchFlow kind="porting-dma-vsync" />

---

## 选择移植层级 {#choosing-a-porting-tier}

根据目标硬件的屏幕接口与内存形态，选择最轻量的接入层级：

| 层级 | 典型硬件场景 | 需要实现的接口 | 帧缓冲所有权 | 输入与硬件加速能力 |
| --- | --- | --- | --- | --- |
| **Tier 1 · 回调帧缓冲 (`FramebufSurface`)** | SPI / QSPI / I2C 小屏（`ST7789`、`ST7735`、`GC9A01`、`ILI9341`） | 无需实现任何 trait，只需传入一个 `\|buf, area: PhysicalRect\|` 闭包 | `FramebufSurface` 内部持有 `Vec<u8>`（`W × H × bpp`） | 适合纯显示或简单轮询；接入触摸芯片与 DMA 同步时可平滑升级至 Tier 2 |
| **Tier 2 · 自定义 `Surface` + `FramebufferAccess`** | 静态 BSS/SRAM DMA 缓冲、RTOS（`NuttX`、`Vela`、`Zephyr`、`FreeRTOS`）、Linux `fbdev` / `DRM` | 实现 `Surface` + `FramebufferAccess`（`framebuffer` 及可选的 `all_buffers`） | 调用方持有的静态数组、`mmap` 显存或双缓冲 DMA 环 | 完整的 `poll_event()` 队列（触控、旋钮、按键、滚轮）+ `SwRendererFactory` 自动双缓冲镜像 |
| **Tier 3 · 2D Blitter / VG-Lite 矢量 GPU** | 2D 位块外设（`STM32 DMA2D`、`NXP PXP`）或 2.5D 矢量 GPU（`VG-Lite`，支持 `vg_lite_query_feature`） | 实现 `RenderEngine<SwRenderer>` + `compose_backend!` + `RendererFactory`（或完整 `Renderer`） | 与 `SwRenderer` 共享帧缓冲，或由设备管理 `BackendTarget` | 硬件加速矩形/位块搬运或贝塞尔路径绘制（`vg_lite_draw`），不支持的特性按命令自动回退至软件光栅化 |

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

        // 1. 发送 CASET (0x2A)，参数为闭区间 [(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]
        // 2. 发送 RASET (0x2B)，参数为闭区间 [(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]
        // 3. 发送 RAMWR (0x2C)，随后通过 SPI DMA 传输窗口像素：
        if x0 == 0 && w == W {
            // 若脏区横跨整屏宽度，直接发起单次连续 DMA 传输：
            let start = y0 as usize * STRIDE;
            let end = (y0 + h) as usize * STRIDE;
            let _contiguous = &buf[start..end];
        } else {
            // 否则按行遍历局部窗口内的像素切片（每行偏移递增 STRIDE）：
            for row in y0..(y0 + h) {
                let offset = row as usize * STRIDE + x0 as usize * BPP;
                let _slice = &buf[offset..offset + w as usize * BPP];
            }
        }
        let _ = (x1, y1);
    },
)
// 当 SPI 片选/指令建立开销较大时，可将多块稀疏脏区合并为单个外接矩形一次性刷新：
.with_redraw_strategy(RedrawStrategy::BoundingBox);

let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();
```

---

## Tier 2：实现 `Surface` 与 `FramebufferAccess` {#tier-2-custom-surface-and-framebuffer}

当你的硬件工程需要：
1. 将帧缓冲放在**静态 BSS / SRAM / PSRAM 段**（32 字节对齐以配合 DMA 与 Cache 行，零堆分配）；
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

#[repr(C, align(32))]
pub struct AlignedFb(pub [u8; FB_LEN]);

pub struct BoardSurface {
    fb: AlignedFb,
    pending_events: [Option<InputEvent>; 8],
    event_head: usize,
    event_len: usize,
    dirty_count: u32,
}

impl BoardSurface {
    pub const fn new() -> Self {
        Self {
            fb: AlignedFb([0; FB_LEN]),
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
            // 对 `area` 所在内存范围执行 Clean D-Cache 并启动 SPI/QSPI DMA。
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
            &mut self.fb.0,
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

<ArchFlow kind="double-buffer-mirror" />

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

### D-Cache 缓存一致性与 DMA 防撕裂屏障 {#dcache-coherency-and-dma}

在启用了数据缓存（D-Cache）的高性能 MCU/SoC（如 **ARM Cortex-M7**、**Cortex-A** 或带缓存的 **RISC-V** 核心）上，CPU 软件光栅化写入的是 D-Cache，而 SPI DMA、LTDC 与 DMA2D 直接通过总线矩阵读写物理 SRAM/PSRAM。请严格遵守以下三条缓存同步规则，防止局部刷新区域出现陈旧横纹或混合花屏：

| 访问交接方向 | 必须执行的屏障操作 | 物理原因 |
| --- | --- | --- |
| **CPU `SwRenderer` → SPI / LTDC DMA 读取** | 启动 DMA 前对脏矩形覆盖的缓存行执行 **Clean D-Cache** (`SCB_CleanDCache_by_Addr`) | 将 CPU 刚画在缓存里的新像素写回物理 SRAM，防止 DMA 传出旧数据 |
| **CPU `SwRenderer` → 2D 加速器 (`DMA2D` / `PXP`)** | 在 `RenderEngine::begin` 中执行 **Clean & Invalidate D-Cache** (`SCB_CleanInvalidateDCache_by_Addr`) | 确保硬件加速器看到之前的 CPU 像素，同时作废即将被硬件覆写的 CPU 缓存行 |
| **2D 加速器 (`DMA2D`) → CPU `SwRenderer`** | 在 `RenderEngine::end` 中等待硬件传输完成标志（TCIF）并执行内存屏障 (`dmb`) | 防止 2D 引擎仍在写内存时 CPU 提前介入读写同一行像素 |

---

## 接入触摸屏、旋钮编码器与物理按键 {#wiring-touch-rotary-and-keys}

`Surface::poll_event(&mut self)` 每次返回一条 `InputEvent`，直到队列排空返回 `None`。所有空间坐标（`PointerDown`、`PointerMove`、`PointerUp`、`Wheel`）均使用**逻辑像素坐标**（`0..display_info().width`、`0..display_info().height`）并以定点数 `Fixed` 表示：
- 当 `display_info().scale` 为 `Fixed::ONE`（`1.0`）时，直接用 `Fixed::from_int(raw_x)` 转换触摸芯片读出的整数坐标；
- 当启用了 HiDPI 缩放（例如 `480×480` 物理屏幕设置 `scale = 2.0`、逻辑视口 `240×240`）时，将物理触摸坐标除以缩放比：`Fixed::from_int(raw_x) / scale`。

### 屏幕旋转 (`MADCTL`) 与触控坐标映射 {#panel-rotation-and-touch-mapping}

当通过屏幕驱动芯片的 `MADCTL (0x36)` 寄存器（`MV`、`MX`、`MY` 位）旋转显示方向时，电容触摸芯片往往仍按原生竖屏方向汇报坐标。在构造 `InputEvent` 前按以下公式将 `(raw_x, raw_y)` 映射为逻辑 `(x, y)`：

| 旋转角度 | `MADCTL` 寄存器位 | 逻辑宽高 `(W, H)` | 触控坐标变换公式 `(raw_x, raw_y) -> (x, y)` |
| --- | --- | --- | --- |
| **0°（原生竖屏）** | `0x00` | `(PHYS_W, PHYS_H)` | `(raw_x, raw_y)` |
| **顺时针 90°（横屏）** | `MV \| MX` (`0x60`) | `(PHYS_H, PHYS_W)` | `(raw_y, PHYS_W - 1 - raw_x)` |
| **180°（倒置竖屏）** | `MX \| MY` (`0xC0`) | `(PHYS_W, PHYS_H)` | `(PHYS_W - 1 - raw_x, PHYS_H - 1 - raw_y)` |
| **顺时针 270°（反向横屏）** | `MV \| MY` (`0xA0`) | `(PHYS_H, PHYS_W)` | `(PHYS_H - 1 - raw_y, raw_x)` |

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

### 中断服务函数 (ISR) 到 UI 主线程的无锁环形队列 {#lock-free-isr-event-queue}

严禁在硬件中断（ISR）内直接调用 `App` 的任何方法。正确做法是在 GPIO / 正交编码器 / 触摸中断中将轻量原始事件推入单生产者单消费者（SPSC）无锁环形队列，唤醒主线程，并在 `Surface::poll_event()` 中排空：

```rust
use core::sync::atomic::{AtomicUsize, Ordering};
use mirui::input::event::input::{InputEvent, KEY_ROTARY_PRESS, Key, Rotary};

#[derive(Clone, Copy)]
pub enum RawIrqEvent {
    RotaryStep(i16),
    CrownPress(bool),
}

pub struct IsrRing<const N: usize> {
    slots: [core::cell::UnsafeCell<Option<RawIrqEvent>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<const N: usize> Sync for IsrRing<N> {}

impl<const N: usize> IsrRing<N> {
    pub const fn new() -> Self {
        Self {
            slots: [const { core::cell::UnsafeCell::new(None) }; N],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// 在 GPIO / 旋钮编码器中断服务函数（ISR，单生产者）中调用：
    pub fn push_from_isr(&self, ev: RawIrqEvent) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % N;
        if next == self.head.load(Ordering::Acquire) {
            return false; // 队列已满
        }
        unsafe { *self.slots[tail].get() = Some(ev) };
        self.tail.store(next, Ordering::Release);
        true
    }

    /// 在 UI 主线程的 `Surface::poll_event(&mut self)`（单消费者）中调用：
    pub fn pop_for_surface(&self) -> Option<InputEvent> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let raw = unsafe { (*self.slots[head].get()).take() }?;
        self.head.store((head + 1) % N, Ordering::Release);
        Some(match raw {
            RawIrqEvent::RotaryStep(delta) => Rotary { id: 0, delta }.into(),
            RawIrqEvent::CrownPress(pressed) => Key {
                code: KEY_ROTARY_PRESS,
                physical: None,
                pressed,
                modifiers: None,
                repeat: Some(false),
            }
            .into(),
        })
    }
}
```

### 休眠生命周期与内存告警钩子 {#lifecycle-and-memory-pressure-hooks}

- **息屏挂起与唤醒恢复（`AppSuspend` / `AppResume`）**：在关闭屏幕背光或进入低功耗睡眠前投递 `InputEvent::AppSuspend`，唤醒后投递 `InputEvent::AppResume`。`App` 会在挂起时自动取消未完成的触摸拖拽手势，并触发 `PersistencePlugin` 将待存状态写入 Flash。
- **内存吃紧清理（`app.memory_warning()`）**：当系统堆水位告警时，调用 `app.memory_warning()` 可立即释放可重建的文本整形、路径排布与离屏渲染缓存，同时完整保留应用业务状态。

---

## Tier 3：用 `compose_backend!` 接入 2D 位块加速与 VG-Lite 矢量 GPU {#tier-3-hardware-acceleration}

嵌入式 SoC 上的图形加速外设通常分为两类，它们的能力边界差异很大，而 `RenderEngine<T>` + `compose_backend!` 可以同时覆盖这两种硬件形态：

| 硬件加速类别 | 典型 IP 与芯片平台 | 原生硬件加速能力 | 动态特性查询与回退边界 |
| --- | --- | --- | --- |
| **固定功能 2D Blitter** | **STM32 Chrom-ART DMA2D**、**NXP PXP**、**Renesas DRW2D**、**ESP32-P4 PPA** | 轴对齐纯色矩形填充（`fill_rect`）、像素格式转换（`RGB565` ↔ `ARGB8888`）与无旋转/仅缩放纹理搬运（`blit`） | 无硬件矢量路径光栅化器与 3D 透视能力——将 `fill_rect` 与 `blit` 路由给硬件，圆角、贝塞尔路径与文字交还 `SwRenderer` |
| **2.5D 矢量与合成 GPU** | **VeriSilicon VG-Lite**（NXP i.MX RT1170 / RT595、恒玄 BES2700 / BES2800、炬芯等） | 硬件二次/三次贝塞尔路径细分与填充（`vg_lite_draw`，支持 `NonZero` / `EvenOdd`）、路径描边（`gcFEATURE_BIT_VG_STROKE_PATH`）、3×3 透视矩阵纹理合成（`vg_lite_blit`）、硬件线性/径向渐变（`gcFEATURE_BIT_VG_LINEAR_GRADIENT_EXT` / `RADIAL_GRADIENT`）与遮罩裁剪（`gcFEATURE_BIT_VG_MASK`） | 不同芯片集成的 VG-Lite IP 裁剪配置不同：在 `RenderEngine::route` 中通过 **`vg_lite_query_feature(feature)`** 查询当前芯片支持的特性位，将受支持的 `fill_path`、`stroke_path`、透视 `blit` 与渐变直接交由 `vg_lite_*` 硬件执行，仅在当前芯片未合成某项扩展特性（或遇到高斯模糊 `ApplyBlur`）时回退至 `SwRenderer` |

借助 `RenderEngine<T>` 与 `compose_backend!`，你只需按外设真实能力实现对应的加速分支：
1. 在 `route(&self, target, request)` 中检查 `DrawRequest`（对于 **VG-Lite** 等可配置 IP，同时结合 `vg_lite_query_feature(gcFEATURE_BIT_VG_*)` 的特性位）；
2. 若当前硬件支持该几何、画刷与混合模式，返回 `Ok(RenderRoute::Native)` 并在 `submit` 中提交给外设（写 `DMA2D` 寄存器或调用 `vg_lite_draw` / `vg_lite_blit`）；
3. 其余操作委托给 `target.route(request)`——`compose_backend!` 会在从硬件引擎切回 `SwRenderer` 时自动调用一次 `end(&mut self, target)`（如等待 `DMA2D TCIF` 标志或调用 `vg_lite_finish()` 提交并等待 GPU 命令缓冲完成），随后由 `SwRenderer` 在同一块帧缓冲上接力绘制。

<ArchFlow kind="compose-backend-route" />

### 示例 A：固定功能 2D 位块加速器 (`DMA2D` / `PXP`) {#example-a-fixed-function-2d-blitter}

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
        // DMA2D 仅加速无旋转、无透视、直角（radius == 0）且不透明的纯色矩形填充：
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
        // 进入连续硬件绘制会话前执行 Clean & Invalidate D-Cache。
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
            // 实际驱动中在此处根据 `target.viewport.physical_rect(...)` 配置 DMA2D OMAR/OOR/NLR 寄存器。
            let _ = (area, color);
        }
        target.submit(draw)
    }

    fn end(&mut self, _target: &mut SwRenderer<'a>) {
        // 在切回 CPU 软件光栅化器访问同一块 RAM 帧缓冲前，轮询等待 DMA2D TCIF 传输完成标志。
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

### 示例 B：支持路径绘制与 `vg_lite_query_feature` 的 VG-Lite 2.5D 矢量 GPU {#example-b-vg-lite-vector-gpu}

与单纯的 2D DMA 位块搬运器不同，**VeriSilicon VG-Lite** 原生支持矢量路径光栅化（将 `M / L / Q / C / Z` 路径编码为 `vg_lite_path_t` 并通过 `vg_lite_draw` 硬件填充或描边）、圆角矩形、3×3 单应性透视矩阵纹理合成（`vg_lite_blit`）以及硬件渐变。由于不同 SoC 厂商在综合 VG-Lite IP 时开启的硬件特性位（`gcFEATURE_BIT_VG_*`）不尽相同，可在初始化时通过 `vg_lite_query_feature` 读取硬件能力表，并将 `fill_path`、`stroke_path`、`fill_rect` 与 `blit` 全部纳入 `compose_backend!` 路由：

```rust
use mirui::render::{
    Canvas, DrawCommand, DrawRequest, Paint, PreparedDraw, RenderEngine, RenderError, RenderRoute,
    SwRenderer,
};
use mirui_macros::compose_backend;

/// 启动时通过 `vg_lite_query_feature(gcFEATURE_BIT_VG_*)` 探测并缓存的硬件特性表。
#[derive(Clone, Copy)]
pub struct VgLiteFeatures {
    pub stroke_path: bool,     // gcFEATURE_BIT_VG_STROKE_PATH != 0
    pub linear_grad_ext: bool, // gcFEATURE_BIT_VG_LINEAR_GRADIENT_EXT != 0
    pub radial_grad: bool,     // gcFEATURE_BIT_VG_RADIAL_GRADIENT != 0
}

pub struct VgLiteEngine {
    pub features: VgLiteFeatures,
    pub hw_paths: u32,
}

impl<'a> RenderEngine<SwRenderer<'a>> for VgLiteEngine {
    fn route(
        &self,
        target: &SwRenderer<'a>,
        request: &DrawRequest<'_, '_>,
    ) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        match request.command {
            // VG-Lite 原生支持二次/三次贝塞尔曲线填充与圆角矩形（根据特性位决定是否硬件加速渐变画刷）：
            DrawCommand::FillPath { paint, .. } => {
                let paint_ok = match paint {
                    Paint::Color(_) => true,
                    Paint::LinearGradient(_) => self.features.linear_grad_ext,
                    Paint::RadialGradient(_) => self.features.radial_grad,
                };
                if paint_ok {
                    Ok(RenderRoute::Native)
                } else {
                    target.route(request)
                }
            }
            DrawCommand::StrokePath { paint, .. } if self.features.stroke_path => match paint {
                Paint::Color(_) => Ok(RenderRoute::Native),
                _ => target.route(request),
            },
            // VG-Lite 的 vg_lite_blit / vg_lite_draw 原生支持 3x3 仿射与透视变换矩阵：
            DrawCommand::Fill { .. } | DrawCommand::Blit { .. } => Ok(RenderRoute::Native),
            // 高斯模糊（ApplyBlur）或当前芯片未开启的扩展特性自动交还 SwRenderer：
            _ => target.route(request),
        }
    }

    fn submit(
        &mut self,
        target: &mut SwRenderer<'a>,
        draw: &PreparedDraw<'_>,
    ) -> Result<(), RenderError> {
        let request = draw.request();
        if self.route(target, &request)? == RenderRoute::Native {
            self.hw_paths += 1;
            // 将 mirui::render::PathCmd 编码为 VLC_OP_MOVE / LINE / QUAD / CUBIC / CLOSE，
            // 并调用 `vg_lite_draw(&vg_target, &vg_path, fill_rule, &matrix, blend, color)`。
        }
        target.submit(draw)
    }

    fn end(&mut self, _target: &mut SwRenderer<'a>) {
        // 调用 `vg_lite_finish()` 提交命令缓冲并等待 GPU 完成后，再允许 CPU 访问同一块帧缓冲。
    }
}

compose_backend! {
    pub struct VgLiteHybrid<'a> {
        sw: SwRenderer<'a>,
        vglite: VgLiteEngine,
    }
    route {
        default => sw,
        fill_rect => vglite,
        fill_path => vglite,
        stroke_path => vglite,
        blit => vglite,
    }
}
```

构造应用时调用 `App::with_factory(surface, Dma2dFactory::default())`（或自定义的 `VgLiteFactory`）即可启用混合加速管线。如果目标平台是拥有独立显存纹理的完整 GPU 后端，请直接实现 `Renderer` 与 `BackendTarget`（详见[各平台渲染目标存储](backend-targets.md)与[混合后端与软件回退](backend-fallback.md)）。

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

### 2. 动态节拍与低功耗睡眠主循环 (`wfi` & RTOS 唤醒) {#_2-tick-less-sleep-and-low-power-main-loop}

在电池供电的手表、手环与传感器终端上，空转轮询 `app.tick()` 会白白消耗电量。每帧 `app.tick()` 后检查 `app.dirty_regions()` 是否为空：当有脏区时调用 `app.render_dirty()` 并按 60 Hz（16 ms）或 VSync 节拍推进；当画面静止无脏区时，立即执行 `wfi`（Wait For Interrupt）或挂起在 RTOS 信号量上，等待下一次触摸/旋钮中断唤醒：

```rust
use mirui::prelude::*;
use mirui::surface::framebuf::FramebufSurface;

let surface = FramebufSurface::new(240, 240, |_buf, _area| {});
let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();

// 低功耗嵌入式事件循环的单轮迭代：
let should_quit = app.tick();
if !should_quit {
    if !app.dirty_regions().unwrap().is_empty() {
        app.render_dirty().unwrap();
        // 活动帧：按 16ms 节拍或等待 VSync / DMA 完成中断。
    } else {
        // 静止帧：零脏区像素！直接执行 cortex_m::asm::wfi() 或阻塞等待输入信号量。
    }
}
```

### 3. Flash 键值存储对接 (`mirui::core::storage::Storage`) {#_2-flash-kv-storage-mirui-core-storage-storage}

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

### 4. I2S / DMA 音频合成 (`AudioMixer` 与 `AudioSink`) {#_3-i2s-dma-audio-synthesis-audiomixer-audiosink}

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

| 阶段 | 验收项 | 常见配置错误表现 | 排查与修正方法 |
| --- | --- | --- | --- |
| **01 · 无头冒烟测试** | 在宿主机 `cargo test` 中将 `#[compose]` 树挂载到 `App::headless(W, H)` | 上板后偶发文本截断或路径超限 | 上板前先在单测中验证布局尺寸与 `Text(capacity: N)` / `PathStore` 容量 |
| **02 · 三色条与字节序** | 并排绘制纯红 `(255,0,0)`、纯绿 `(0,255,0)`、纯蓝 `(0,0,255)` 色块 | 红蓝颠倒，或绿色条出现高低位断裂杂色 | 红蓝颠倒切换 `MADCTL` 的 BGR 位；绿色断裂在 `RGB565` 与 `RGB565Swapped` 间切换 |
| **03 · 局部脏区跨距** | 点击独立小按钮触发 `area.width() < W` 的局部刷新 | 局部重绘区域变成斜纹或画面错位 | 确认 `CASET/RASET` 终点坐标为闭区间 `(x+w-1, y+h-1)`，且逐行偏移按 `W * bpp` 递增 |
| **04 · D-Cache 与 DMA 同步** | 快速拖动滑块或滚动列表 | 局部脏矩形内出现水平陈旧像素条纹 | 启动 DMA 前执行 `SCB_CleanDCache_by_Addr`，并在 `begin_flush()` 中等待上一帧 DMA 完成 |
| **05 · 触控旋转与空闲功耗** | 依次点击屏幕四角按钮，随后静置屏幕 | 触控坐标镜像/旋转错位；静止时电流偏高 | 对照 `MADCTL` 触控变换表修正 `(x, y)`，并在 `dirty_regions().is_empty()` 时进入 `wfi` 休眠 |
