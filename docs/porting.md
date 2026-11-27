# Porting a display and input backend {#porting-a-display-and-input-backend}

`mirui` separates the scene graph, layout engine, and reactive models from platform hardware through four orthogonal boundaries:
- **`Surface`**: reports display metrics (`DisplayInfo`), drains input events (`InputEvent`), and flushes damaged physical-pixel rectangles (`PhysicalRect`);
- **`FramebufferAccess` & `RendererFactory`**: exposes single or rotating CPU framebuffers to `SwRendererFactory` (with automatic scroll pixel-copy and dirty-region mirroring) or binds a custom GPU renderer;
- **`RenderEngine` & `compose_backend!`**: intercepts hardware-supported 2D operations (such as STM32 Chrom-ART DMA2D, NXP PXP, or VG-Lite) while delegating unsupported rounded, projective, or blended draws to the software rasterizer;
- **`set_clock`, `Storage`, and `AudioSink`**: connects bare-metal timers, flash key-value stores, and I2S/DMA audio buffers on `no_std`.

<MiruiSnippet recipe="porting-inspector" label="Surface & Dirty Window" file="recipes.rs" height="220px">

<template #exercise>

Click `Touch Tap` or `Rotary +1` to move the damaged `PhysicalRect` window, or toggle `RGB565 (2B)` / `RGBA8 (4B)` to see how `flush(area)` computes the exact `CASET` / `RASET` column/row bounds and transfer byte count.

</template>

</MiruiSnippet>

<ArchFlow kind="porting-dma-vsync" />

---

## Choosing a porting tier {#choosing-a-porting-tier}

Pick the lightest tier that matches your display controller and memory architecture:

| Tier | Typical Target | What You Implement | Framebuffer Ownership | Input & Hardware Acceleration |
| --- | --- | --- | --- | --- |
| **Tier 1 · Callback Framebuffer** | SPI / QSPI / I2C LCD (`ST7789`, `ST7735`, `GC9A01`, `ILI9341`) | Pass one `\|buf, area: PhysicalRect\|` closure to `FramebufSurface` | `FramebufSurface` allocates one `Vec<u8>` (`W × H × bpp`) | Feed events directly or upgrade to Tier 2 when adding a touch IC |
| **Tier 2 · Custom `Surface` + `FramebufferAccess`** | Static BSS DMA buffers, RTOS (`NuttX`, `Vela`, `Zephyr`, `FreeRTOS`), Linux `fbdev` / `DRM` | `Surface` + `FramebufferAccess` (`framebuffer` & optional `all_buffers`) | Caller-owned static array, mmap region, or DMA scanout ring | Full `poll_event()` queue (`Pointer`, `Rotary`, `Key`, `Wheel`) + automatic multi-buffer dirty mirroring |
| **Tier 3 · Hybrid 2D Engine or Custom GPU** | MCU 2D engines (`STM32 DMA2D`, `NXP PXP`, `VG-Lite`) or native GPU APIs | `RenderEngine<SwRenderer>` + `compose_backend!` + `RendererFactory` (or full `Renderer`) | Shared with `SwRenderer` or device-owned `BackendTarget` | Hardware-accelerated fills/blits with automatic software fallback for unsupported geometry |

---

## Tier 1: Bring up an SPI panel with `FramebufSurface` {#tier-1-callback-framebuffer}

If your board has an allocator (`extern crate alloc`) and pushes pixels to an external display controller over SPI or 8080 parallel bus, `FramebufSurface` gets a panel running without implementing any traits.

### Pixel formats and SPI byte order {#pixel-formats-and-spi-byte-order}

Most SPI LCD controllers (`ST7789`, `ST7735`, `GC9A01`, `ILI9341`) expect 16-bit RGB565 pixels transmitted **MSB first** (big-endian), whereas ARM Cortex-M and RISC-V MCUs are little-endian. Choose `ColorFormat::RGB565Swapped` so `SwRenderer` writes byte-swapped RGB565 directly during rasterization—your SPI DMA transfer can then stream the slice without a byte-swapping loop:

| `ColorFormat` | Bytes / Pixel | Memory Layout | Typical Panel / Bus |
| --- | --- | --- | --- |
| `RGB565Swapped` | `2` | `[msb, lsb]` (big-endian RGB565) | SPI / QSPI panels (`ST7789`, `GC9A01`, `ILI9341`) on little-endian MCUs |
| `RGB565` | `2` | `[lsb, msb]` (native little-endian) | Parallel RGB / LTDC / MIPI DSI / Linux `fbdev` 16-bpp |
| `RGB888` | `3` | `[r, g, b]` packed | 24-bit parallel RGB or MIPI DSI |
| `RGBA8888` | `4` | `[r, g, b, a]` | Desktop / Web Canvas / GPU upload targets |
| `BGRA8888` | `4` | `[b, g, r, a]` | DRM / X11 / Windows native scanout buffers |

### Slicing the dirty rectangle for `CASET` / `RASET` {#slicing-the-dirty-rectangle-for-caset-raset}

During `app.render_dirty()` (or `app.tick()`), `FramebufSurface` invokes your callback once per damaged physical rectangle (`area: PhysicalRect`). Note that `buf` is the **full** `W × H` framebuffer with row stride `W * bpp` bytes; when `area.width() < W`, copy or DMA each row slice of the sub-window:

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

        // 1. Send CASET (0x2A): [(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]
        // 2. Send RASET (0x2B): [(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]
        // 3. Send RAMWR (0x2C) and stream the window bytes:
        if x0 == 0 && w == W {
            // Full-width dirty band: stream one contiguous slice over SPI DMA.
            let start = y0 as usize * STRIDE;
            let end = (y0 + h) as usize * STRIDE;
            let _contiguous = &buf[start..end];
        } else {
            // Sub-width dirty box: advance by STRIDE for each row.
            for row in y0..(y0 + h) {
                let offset = row as usize * STRIDE + x0 as usize * BPP;
                let _slice = &buf[offset..offset + w as usize * BPP];
            }
        }
        let _ = (x1, y1);
    },
)
// Coalesce sparse dirty rects into one bounding box when SPI CS/command overhead is high:
.with_redraw_strategy(RedrawStrategy::BoundingBox);

let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();
```

---

## Tier 2: Implement `Surface` and `FramebufferAccess` {#tier-2-custom-surface-and-framebuffer}

When you need to:
1. Place the framebuffer in a **static BSS / SRAM / PSRAM section** (32-byte aligned for DMA & cache lines, with zero heap allocation);
2. Deliver touch, rotary encoder, or keypad events from `poll_event()`;
3. Coordinate **begin / flush / end** transactions with an asynchronous DMA channel or VSync interrupt; or
4. Rotate across **double or triple scanout buffers**,

implement `Surface` and `FramebufferAccess` directly on your board struct. Because `SwRendererFactory` provides a blanket implementation for every `B: FramebufferAccess`, `App::new(your_surface)` works out of the box.

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
        // Keep root UI clear of rounded watch corners or camera cutouts:
        SafeAreaInsets {
            top: Fixed::from_int(12),
            right: Fixed::ZERO,
            bottom: Fixed::from_int(12),
            left: Fixed::ZERO,
        }
    }

    fn begin_flush(&mut self) -> Result<(), RenderError> {
        // Wait for any previous async SPI DMA transfer to finish before mutating or flushing.
        Ok(())
    }

    fn flush(&mut self, area: PhysicalRect) -> Result<(), RenderError> {
        if !area.is_empty() {
            self.dirty_count += 1;
            // Clean D-Cache for `area` rows and start SPI/QSPI DMA transfer.
        }
        Ok(())
    }

    fn end_flush(&mut self) -> Result<(), RenderError> {
        // Commit the frame (e.g. deassert CS or trigger panel refresh).
        Ok(())
    }

    fn abort_flush(&mut self) {
        // Called if rendering fails mid-frame; pending damage is retained by App.
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

### Double-buffered scanout and automatic mirroring {#double-buffered-scanout-and-mirroring}

When driving a parallel RGB / LTDC / MIPI DSI / Linux DRM controller that scans out directly from RAM, writing into the currently scanned buffer causes tearing. With two buffers (`front` being scanned by the display controller and `back` being drawn by `mirui`), partial redraws require keeping both buffers in sync.

You do **not** need to copy the entire screen on every frame or manually track dirty rectangles. Override `Surface::buffer_count` to return `2` and `FramebufferAccess::all_buffers` to return `[completed_draw_buffer, inactive_mirror_buffer]` after `end_flush()` swaps the buffers. `SwRendererFactory::mirror_buffers` will automatically:
1. Apply any in-place scroll pixel shifts (`plan.shifts`) to the inactive buffer first;
2. Copy only the redrawn physical rectangles (`redraw`) from the just-completed buffer into the inactive buffer.

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
        // Flip scanout to `self.draw_index` at VSync, then switch the writable slot:
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
        // Called right after `end_flush()` succeeds:
        // The completed frame (`last_presented`) MUST come first, followed by writable inactive buffers.
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

### D-Cache coherency and DMA tearing prevention {#dcache-coherency-and-dma}

On MCUs with a data cache (such as **ARM Cortex-M7**, **Cortex-A**, or cached **RISC-V** cores), CPU software rasterization writes into the CPU's D-Cache, while SPI DMA, LTDC, and DMA2D access physical SRAM/PSRAM directly over the bus matrix. Follow these three cache-maintenance rules to prevent stale horizontal lines or corrupted blends:

| Transition | Required Barrier | Why |
| --- | --- | --- |
| **CPU `SwRenderer` → SPI / LTDC DMA read** | **Clean D-Cache** (`SCB_CleanDCache_by_Addr`) over the dirty `PhysicalRect` span before starting DMA | Flushes dirty cache lines out to SRAM so the DMA controller does not transmit stale bytes |
| **CPU `SwRenderer` → 2D Accelerator (`DMA2D` / `PXP`)** | **Clean & Invalidate D-Cache** (`SCB_CleanInvalidateDCache_by_Addr`) in `RenderEngine::begin` | Ensures hardware sees previous CPU pixels AND discards CPU cache lines that hardware is about to overwrite |
| **2D Accelerator (`DMA2D`) → CPU `SwRenderer`** | Wait for hardware transfer-complete IRQ/flag in `RenderEngine::end` + memory barrier (`dmb`) | Prevents the CPU from reading or writing the same pixel rows while the 2D engine is still active |

---

## Wiring touch, rotary encoders, and hardware keys {#wiring-touch-rotary-and-keys}

`Surface::poll_event(&mut self)` drains one `InputEvent` at a time until it returns `None`. All spatial coordinates (`PointerDown`, `PointerMove`, `PointerUp`, `Wheel`) are in **logical pixels** (`0..display_info().width`, `0..display_info().height`) represented as `Fixed`:
- If `display_info().scale` is `Fixed::ONE` (`1.0`), convert raw touch controller integers directly with `Fixed::from_int(raw_x)`;
- If `scale` is `2.0` (for example, a `480×480` physical panel configured as a `240×240` logical viewport), divide physical touch coordinates by `scale`: `Fixed::from_int(raw_x) / scale`.

### Panel rotation (`MADCTL`) and touch coordinate transforms {#panel-rotation-and-touch-mapping}

When a panel is rotated in hardware via its `MADCTL (0x36)` register (`MV`, `MX`, `MY` bits), the touch controller often still reports coordinates in the sensor's native portrait orientation. Transform `(raw_x, raw_y)` into logical `(x, y)` before constructing `InputEvent`:

| Rotation | `MADCTL` Bits | Logical `(W, H)` | Touch Coordinate Transform `(raw_x, raw_y) -> (x, y)` |
| --- | --- | --- | --- |
| **0° (Portrait)** | `0x00` | `(PHYS_W, PHYS_H)` | `(raw_x, raw_y)` |
| **90° Clockwise** | `MV \| MX` (`0x60`) | `(PHYS_H, PHYS_W)` | `(raw_y, PHYS_W - 1 - raw_x)` |
| **180° Inverted** | `MX \| MY` (`0xC0`) | `(PHYS_W, PHYS_H)` | `(PHYS_W - 1 - raw_x, PHYS_H - 1 - raw_y)` |
| **270° Clockwise** | `MV \| MY` (`0xA0`) | `(PHYS_H, PHYS_W)` | `(PHYS_H - 1 - raw_y, raw_x)` |

```rust
use mirui::input::event::input::{
    InputEvent, KEY_HW_BUTTON_0, KEY_RETURN, KEY_ROTARY_PRESS, Key, PointerKind, Rotary,
};
use mirui::types::Fixed;

/// Capacitive touch sample (e.g. FT6336 / CST816 / GT911).
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

/// Detent delta from a quadrature encoder or smart-watch crown (+1 clockwise, -1 counter-clockwise).
pub fn map_rotary_detent(delta: i16) -> InputEvent {
    Rotary { id: 0, delta }.into()
}

/// Crown push button or side GPIO hardware button (`0..N`).
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

/// Standard navigation / confirm key for keypad-driven UIs.
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

### Lock-free ISR-to-UI event queue {#lock-free-isr-event-queue}

Never call `App` methods inside a hardware interrupt service routine (ISR). Instead, push compact raw samples from your GPIO / I2C / timer ISR into a single-producer single-consumer (SPSC) ring buffer (or `critical_section`-guarded queue), wake the main thread, and drain them inside `Surface::poll_event()`:

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

    /// Called from GPIO / quadrature encoder ISR (single producer):
    pub fn push_from_isr(&self, ev: RawIrqEvent) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % N;
        if next == self.head.load(Ordering::Acquire) {
            return false; // Queue full
        }
        unsafe { *self.slots[tail].get() = Some(ev) };
        self.tail.store(next, Ordering::Release);
        true
    }

    /// Called inside `Surface::poll_event(&mut self)` on the UI thread (single consumer):
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

### Lifecycle and memory-pressure hooks {#lifecycle-and-memory-pressure-hooks}

- **Suspend & Resume**: Enqueue `InputEvent::AppSuspend` before powering down the display or entering deep sleep, and `InputEvent::AppResume` after waking up. `App` automatically cancels in-flight pointer drags/gestures and triggers `PersistencePlugin` save hooks on suspend.
- **Memory warnings**: When heap headroom runs low, call `app.memory_warning()` to trim reconstructible text-shaping, path-placement, and offscreen caches without losing application state.

---

## Tier 3: 2D hardware acceleration with `compose_backend!` {#tier-3-hardware-acceleration}

Many embedded SoCs include a 2D DMA blitter (such as **STM32 Chrom-ART DMA2D**, **NXP PXP**, **Renesas DRW2D**, or **Vela VG-Lite**) that can fill axis-aligned solid rectangles and blit unrotated textures at bus speed, but cannot rasterize cubic Bézier paths, SDF glyphs, or projective 3D quads.

With `RenderEngine<T>` and `compose_backend!`, you only write the fast path for the commands your peripheral supports. For every draw call:
1. Your engine's `route(&self, target, request)` inspects the `DrawRequest`;
2. If the hardware supports the exact geometry, paint, and blend mode, `route` returns `Ok(RenderRoute::Native)` and `submit` drives the peripheral;
3. For everything else (rounded corners, arbitrary paths, rotated quads), `route` delegates to `target.route(request)` or returns `ExactFallback`, and `compose_backend!` automatically runs `end(&mut self, target)` (synchronizing DMA completion) before letting `SwRenderer` draw directly into the shared framebuffer.

<ArchFlow kind="compose-backend-route" />

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
        // Only accelerate unrotated, non-projective, sharp-cornered opaque rectangle fills:
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
        // Delegate rounded rects, paths, text, and blits to SwRenderer:
        target.route(request)
    }

    fn begin(&mut self, _target: &mut SwRenderer<'a>) -> Result<(), RenderError> {
        // Clean & invalidate D-Cache before DMA2D writes into shared SRAM/PSRAM.
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
            // In production, program DMA2D OMAR / OOR / NLR with `target.viewport.physical_rect(...)`.
            let _ = (area, color);
        }
        target.submit(draw)
    }

    fn end(&mut self, _target: &mut SwRenderer<'a>) {
        // Poll DMA2D TCIF (Transfer Complete Interrupt Flag) before CPU software rasterization resumes.
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

Pass the custom factory to `App::with_factory(surface, Dma2dFactory::default())`. For full GPU backends that own device textures rather than a CPU framebuffer, implement `Renderer` + `BackendTarget` directly; see [Backend target storage](backend-targets.md) and [Exact backend fallback](backend-fallback.md).

---

## Bare-metal clocks, flash storage, and audio {#clocks-storage-and-audio}

### 1. Monotonic clock (`mirui::core::time::set_clock`) {#_1-monotonic-clock-mirui-core-time-set-clock}

On `std`, `mirui` automatically anchors a monotonic clock on first read. On `no_std`, register a nanosecond clock function during board initialization and install a plugin that updates `MonoClock` so spring transitions, tweens, long-press gesture thresholds, and `#[timer]` schedules advance in real time:

```rust
use mirui::app::{App, Plugin};
use mirui::core::time;
use mirui::ecs::MonoClock;

fn board_uptime_ns() -> u64 {
    // Read hardware timer / DWT cycle counter / RTOS tick converted to nanoseconds:
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

### 2. Tick-less sleep and low-power main loop (`wfi` & RTOS pacing) {#_2-tick-less-sleep-and-low-power-main-loop}

On battery-powered wearables and sensor nodes, spinning `app.tick()` in a busy loop wastes power. Check whether `app.dirty_regions()` is non-empty or an animation/spring is active; when the scene is quiescent, block on an RTOS semaphore or execute `wfi` (Wait For Interrupt) until the next touch interrupt or timer deadline:

```rust
use mirui::prelude::*;
use mirui::surface::framebuf::FramebufSurface;

let surface = FramebufSurface::new(240, 240, |_buf, _area| {});
let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();

// Single iteration of a low-power embedded event loop:
let should_quit = app.tick();
if !should_quit {
    if !app.dirty_regions().unwrap().is_empty() {
        app.render_dirty().unwrap();
        // Active frame: pace to 60 Hz (16 ms) or wait for VSync / DMA completion.
    } else {
        // Idle frame: zero dirty pixels! Enter WFI or wait on IRQ semaphore.
    }
}
```

### 3. Flash / KV storage (`mirui::core::storage::Storage`) {#_2-flash-kv-storage-mirui-core-storage-storage}

To persist `#[model(persistence)]` state to SPI NorFlash, LittleFS, NVS, or EEPROM, implement the synchronous `Storage` trait and pass it to `PersistencePlugin::new(storage)`. Return `WriteOutcome::Unchanged` when the stored bytes already match `value` to avoid redundant flash erase/program cycles:

```rust
use alloc::vec::Vec;
use mirui::core::storage::{Storage, StorageError, WriteOutcome};

pub struct FlashKvStorage;

impl Storage for FlashKvStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let _ = key;
        // Return Ok(None) only when the key does not exist; return Err(StorageError::Io) on CRC/bus errors.
        Ok(None)
    }

    fn write(&mut self, key: &str, value: &[u8]) -> Result<WriteOutcome, StorageError> {
        if self.read(key)?.as_deref() == Some(value) {
            return Ok(WriteOutcome::Unchanged);
        }
        // Write `value` to flash/KV sector for `key`:
        Ok(WriteOutcome::Written)
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let _ = key;
        Ok(())
    }
}
```

### 4. I2S / DMA audio synthesis (`AudioMixer` & `AudioSink`) {#_3-i2s-dma-audio-synthesis-audiomixer-audiosink}

When the `audio` feature is enabled, `AudioMixer<V>` synthesizes polyphonic waveforms and scores into caller-owned `i16` PCM slices with zero heap allocation. Drive `mixer.render_mono(&mut dma_half_buffer)` from an I2S DMA interrupt or RTOS audio task, and implement `AudioSink` to forward `AudioCommand`s from `app.audio()`:

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

    /// Call from I2S DMA half-transfer / transfer-complete ISR or audio thread:
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

## Porting verification checklist {#porting-checklist}

| Step | Verification Target | Symptom if Misconfigured | Fix |
| --- | --- | --- | --- |
| **01 · Headless Smoke Test** | Mount `#[compose]` tree on `App::headless(W, H)` in `cargo test` | Panic or truncated text on device | Increase `Text(capacity: N)` or `PathStore` capacity before flashing hardware |
| **02 · Colorbar & Endianness** | Draw red, green, and blue `View` bars (`(255,0,0)`, `(0,255,0)`, `(0,0,255)`) | Red and blue swapped, or green bar has striped noise | Swap `RGB565` ↔ `RGB565Swapped` (byte order) or toggle panel `MADCTL` BGR bit |
| **03 · Sub-Window Stride** | Tap a small button so `flush(area)` receives `area.width() < W` | Partial update appears diagonal or sheared | Ensure `CASET/RASET` end coords are inclusive `(x+w-1, y+h-1)` and row offset advances by `W * bpp` |
| **04 · D-Cache & DMA Sync** | Animate a fast-moving slider or scroll list | Horizontal stale-pixel tearing bands inside dirty rect | Call `SCB_CleanDCache_by_Addr` before DMA and wait for DMA completion in `begin_flush()` |
| **05 · Touch & Idle Sleep** | Tap buttons in all 4 corners, then leave screen idle | Corner taps inverted; high idle current draw | Apply the `MADCTL` touch transform table and enter `wfi` when `dirty_regions().is_empty()` |
