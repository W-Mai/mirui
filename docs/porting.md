# Porting a display and input backend {#porting-a-display-and-input-backend}

`mirui` separates the scene graph, layout engine, and reactive models from platform hardware through four orthogonal boundaries:
- **`Surface`**: reports display metrics (`DisplayInfo`), polls input events (`InputEvent`), and flushes damaged physical-pixel rectangles (`PhysicalRect`);
- **`FramebufferAccess` & `RendererFactory`**: exposes single or rotating CPU framebuffers to `SwRendererFactory` (with automatic scroll pixel-copy and dirty-region mirroring) or binds a custom GPU renderer;
- **`RenderEngine` & `compose_backend!`**: intercepts hardware-supported 2D operations (such as STM32 Chrom-ART DMA2D, NXP PXP, or VG-Lite) while delegating unsupported rounded, projective, or blended draws to the software rasterizer;
- **`set_clock`, `Storage`, and `AudioSink`**: connects bare-metal timers, flash key-value stores, and I2S/DMA audio buffers on `no_std`.

<MiruiSnippet recipe="porting-inspector" label="Surface & Dirty Window" file="recipes.rs" height="220px">

<template #exercise>

Click `Touch Tap` or `Rotary +1` to move the damaged `PhysicalRect` window, or toggle `RGB565 (2B)` / `RGBA8 (4B)` to see how `flush(area)` computes the exact `CASET` / `RASET` column/row bounds and transfer byte count.

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="Four-stage backend porting boundary from host loop and clock through Surface and RenderEngine to the panel or GPU">
  <svg viewBox="0 0 880 182" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="port-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="192" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · HOST &amp; CLOCK</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Main Loop &amp; Time</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">time::set_clock(ns_fn)</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">app.tick() / render_dirty()</text>
    <text x="32" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Drives springs, timers &amp; input</text>
    <path d="M208 91H246" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="254" y="22" width="198" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="270" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · SURFACE &amp; INPUT</text>
    <text x="270" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Display &amp; Peripherals</text>
    <text x="270" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">display_info() · poll_event()</text>
    <text x="270" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">begin_flush → flush(area)</text>
    <text x="270" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Touch, Rotary &amp; HW keys</text>
    <path d="M452 91H490" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="498" y="22" width="194" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="514" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · RENDER PIPELINE</text>
    <text x="514" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">SW, Hybrid or GPU</text>
    <text x="514" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">FramebufferAccess</text>
    <text x="514" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">compose_backend! (DMA2D)</text>
    <text x="514" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Scroll shifts &amp; mirror_buffers</text>
    <path d="M692 91H730" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#port-arrow)" />
    <rect x="738" y="22" width="126" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="754" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · HARDWARE</text>
    <text x="754" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Panel / Scanout</text>
    <text x="754" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">SPI CASET/RASET</text>
    <text x="754" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">DRM / VSync Flip</text>
    <text x="754" y="138" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Only dirty pixels</text>
  </svg>
</div>

## Contents {#contents}

1. [Choosing a porting tier](#choosing-a-porting-tier)
2. [Tier 1: Bring up an SPI panel with `FramebufSurface`](#tier-1-callback-framebuffer)
3. [Tier 2: Implement `Surface` and `FramebufferAccess`](#tier-2-custom-surface-and-framebuffer)
   - [Double-buffered scanout and automatic mirroring](#double-buffered-scanout-and-mirroring)
4. [Wiring touch, rotary encoders, and hardware keys](#wiring-touch-rotary-and-keys)
5. [Tier 3: 2D hardware acceleration with `compose_backend!`](#tier-3-hardware-acceleration)
6. [Bare-metal clocks, flash storage, and audio](#clocks-storage-and-audio)
7. [Porting verification checklist](#porting-checklist)

---

## Choosing a porting tier {#choosing-a-porting-tier}

Pick the lightest tier that matches your display controller and memory architecture:

| Tier | Typical Target | What You Implement | Framebuffer Ownership | Input & Hardware Acceleration |
| --- | --- | --- | --- | --- |
| **Tier 1 · Callback Framebuffer** | SPI / QSPI / I2C LCD (`ST7789`, `ST7735`, `GC9A01`, `ILI9341`) | Pass one `\|buf, area: PhysicalRect\|` closure to `FramebufSurface` | `FramebufSurface` allocates one `Vec<u8>` (`W × H × bpp`) | Feed events directly or upgrade to Tier 2 when adding a touch IC |
| **Tier 2 · Custom `Surface` + `FramebufferAccess`** | Static BSS DMA buffers, RTOS (`NuttX`, `Zephyr`, `FreeRTOS`), Linux `fbdev` / `DRM` | `Surface` + `FramebufferAccess` (`framebuffer` & optional `all_buffers`) | Caller-owned static array, mmap region, or DMA scanout ring | Full `poll_event()` queue (`Pointer`, `Rotary`, `Key`, `Wheel`) + automatic multi-buffer dirty mirroring |
| **Tier 3 · Hybrid 2D Engine or Custom GPU** | MCU 2D engines (`STM32 DMA2D`, `NXP PXP`, `VG-Lite`) or native GPU APIs | `RenderEngine<SwRenderer>` + `compose_backend!` + `RendererFactory` (or full `Renderer`) | Shared with `SwRenderer` or device-owned `BackendTarget` | Hardware-accelerated fills/blits with automatic software fallback for unsupported geometry |

---

## Tier 1: Bring up an SPI panel with `FramebufSurface` {#tier-1-callback-framebuffer}

If your board has an allocator (`extern crate alloc`) and pushes pixels to an external display controller over SPI or 8080 parallel bus, `FramebufSurface` gets a panel running without implementing any traits.

### Pixel formats and SPI byte order

Most SPI LCD controllers (`ST7789`, `ST7735`, `GC9A01`, `ILI9341`) expect 16-bit RGB565 pixels transmitted **MSB first** (big-endian), whereas ARM Cortex-M and RISC-V MCUs are little-endian. Choose `ColorFormat::RGB565Swapped` so `SwRenderer` writes byte-swapped RGB565 directly during rasterization—your SPI DMA transfer can then blast the slice without a byte-swapping loop:

| `ColorFormat` | Bytes / Pixel | Memory Layout | Typical Panel / Bus |
| --- | --- | --- | --- |
| `RGB565Swapped` | `2` | `[msb, lsb]` (big-endian RGB565) | SPI / QSPI panels (`ST7789`, `GC9A01`, `ILI9341`) on little-endian MCUs |
| `RGB565` | `2` | `[lsb, msb]` (native little-endian) | Parallel RGB / LTDC / MIPI DSI / Linux `fbdev` 16-bpp |
| `RGB888` | `3` | `[r, g, b]` packed | 24-bit parallel RGB or MIPI DSI |
| `RGBA8888` | `4` | `[r, g, b, a]` | Desktop / Web Canvas / GPU upload targets |
| `BGRA8888` | `4` | `[b, g, r, a]` | DRM / X11 / Windows native scanout buffers |

### Slicing the dirty rectangle for `CASET` / `RASET`

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

fn push_spi_window(
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
    row_bytes: impl Iterator<Item = &'static [u8]>,
) {
    let _ = (x0, y0, x1, y1, row_bytes);
    // 1. Send CASET (0x2A) with [x0 >> 8, x0 as u8, x1 >> 8, x1 as u8]
    // 2. Send RASET (0x2B) with [y0 >> 8, y0 as u8, y1 >> 8, y1 as u8]
    // 3. Send RAMWR (0x2C) and stream each row slice over SPI DMA
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

        // If the dirty rect spans the full panel width, send one contiguous DMA slice:
        if x0 == 0 && w == W {
            let start = y0 as usize * STRIDE;
            let end = (y0 + h) as usize * STRIDE;
            let _contiguous = &buf[start..end];
        } else {
            // Otherwise iterate row by row across the sub-window:
            for row in y0..(y0 + h) {
                let offset = row as usize * STRIDE + x0 as usize * BPP;
                let _slice = &buf[offset..offset + w as usize * BPP];
            }
        }
        let _ = (x1, y1);
    },
)
// Optional: coalesce fragmented dirty rects into one bounding box per frame
// when SPI command/CS setup overhead outweighs transferring a few extra pixels.
.with_redraw_strategy(RedrawStrategy::BoundingBox);

let mut app = App::new(surface);
app.with_default_widgets().with_default_systems();
let _root = app.spawn_root().id();
app.render().unwrap();
```

---

## Tier 2: Implement `Surface` and `FramebufferAccess` {#tier-2-custom-surface-and-framebuffer}

When you need to:
1. Place the framebuffer in a **static BSS / SRAM section** (without heap-allocating a `Vec<u8>`);
2. Deliver touch, rotary encoder, or keypad events from `poll_event()`;
3. Coordinate **begin / flush / end** transactions with an异步 DMA channel or VSync interrupt; or
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
            // Push `area` from `self.fb` to the display controller.
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

### Double-buffered scanout and automatic mirroring {#double-buffered-scanout-and-mirroring}

When driving a parallel RGB / MIPI DSI / Linux DRM controller that scans out directly from RAM, writing into the currently scanned buffer causes tearing. With two buffers (`front` being scanned by the display controller and `back` being drawn by `mirui`), partial redraws require keeping both buffers in sync.

You do **not** need to copy the entire screen on every frame or manually track dirty rectangles. Override `Surface::buffer_count` to return `2` and `FramebufferAccess::all_buffers` to return `[completed_draw_buffer, inactive_mirror_buffer]` after `end_flush()` swaps the buffers. `SwRendererFactory::mirror_buffers` will automatically:
1. Apply any in-place scroll pixel shifts (`plan.shifts`) to the inactive buffer first;
2. Copy only the redrawn physical rectangles (`redraw`) from the just-completed buffer into the inactive buffer.

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

---

## Wiring touch, rotary encoders, and hardware keys {#wiring-touch-rotary-and-keys}

`Surface::poll_event(&mut self)` drains one `InputEvent` at a time until it returns `None`. All spatial coordinates (`PointerDown`, `PointerMove`, `PointerUp`, `Wheel`) are in **logical pixels** (`0..display_info().width`, `0..display_info().height`) represented as `Fixed`:
- If `display_info().scale` is `Fixed::ONE` (`1.0`), convert raw touch controller integers directly with `Fixed::from_int(raw_x)`;
- If `scale` is `2.0` (for example, a `480×480` physical panel configured as a `240×240` logical viewport), divide physical touch coordinates by `scale`: `Fixed::from_int(raw_x) / scale`.

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

### Lifecycle and memory-pressure hooks

- **Suspend & Resume**: Enqueue `InputEvent::AppSuspend` before powering down the display or entering deep sleep, and `InputEvent::AppResume` after waking up. `App` automatically cancels in-flight pointer drags/gestures and triggers `PersistencePlugin` save hooks on suspend.
- **Memory warnings**: When heap headroom runs low, call `app.memory_warning()` to trim reconstructible text-shaping, path-placement, and offscreen caches without losing application state.

---

## Tier 3: 2D hardware acceleration with `compose_backend!` {#tier-3-hardware-acceleration}

Many embedded SoCs include a 2D DMA blitter (such as **STM32 Chrom-ART DMA2D**, **NXP PXP**, **Renesas DRW2D**, or **Vela VG-Lite**) that can fill axis-aligned solid rectangles and blit unrotated textures at bus speed, but cannot rasterize cubic Bézier paths, SDF glyphs, or projective 3D quads.

With `RenderEngine<T>` and `compose_backend!`, you only write the fast path for the commands your peripheral supports. For every draw call:
1. Your engine's `route(&self, target, request)` inspects the `DrawRequest`;
2. If the hardware supports the exact geometry, paint, and blend mode, `route` returns `Ok(RenderRoute::Native)` and `submit` drives the peripheral;
3. For everything else (rounded corners, arbitrary paths, rotated quads), `route` delegates to `target.route(request)` or returns `ExactFallback`, and `compose_backend!` automatically runs `end(&mut self, target)` (synchronizing DMA completion) before letting `SwRenderer` draw directly into the shared framebuffer.

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
        // Prepare peripheral registers for a batch of hardware draws.
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
            // Here we forward to the software target to keep the example self-contained:
            let _ = (area, color);
        }
        target.submit(draw)
    }

    fn end(&mut self, _target: &mut SwRenderer<'a>) {
        // Wait for DMA2D transfer-complete flag before CPU software rasterization touches RAM.
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

### 1. Monotonic clock (`mirui::core::time::set_clock`)

On `std`, `mirui` automatically anchors a monotonic clock on first read. On `no_std`, register a nanosecond clock function during board initialization and install a plugin that updates `MonoClock` (or use `StdInstantClockPlugin` where available) so spring transitions, tweens, long-press gesture thresholds, and `#[timer]` schedules advance in real time:

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

### 2. Flash / KV storage (`mirui::core::storage::Storage`)

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

### 3. I2S / DMA audio synthesis (`AudioMixer` & `AudioSink`)

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

1. **Headless smoke test**: Before flashing hardware, mount your application `#[compose]` tree on `App::headless(W, H)` and run `app.render().unwrap()` in `cargo test` to verify layout and font/path capacities.
2. **Colorbar & endianness check**: Render three `View` bars (`Color::rgb(255, 0, 0)`, `Color::rgb(0, 255, 0)`, `Color::rgb(0, 0, 255)`). If red and blue are swapped, switch between `RGB565` and `RGB565Swapped` (or toggle the panel's `MADCTL` BGR bit); if green is split across high/low bytes, swap `RGB565` ↔ `RGB565Swapped`.
3. **Sub-window stride check**: Trigger a small button state change so `flush(area)` receives a partial `PhysicalRect` (`area.width() < W`). If the updated region appears skewed or diagonal on screen, check that your `CASET`/`RASET` inclusive end coordinates are `(x + w - 1, y + h - 1)` and that each row advances by `W * bpp` bytes in the source buffer.
4. **Touch & rotary alignment**: Place a `Button` in each corner of the screen and verify that `PointerDown` / `PointerUp` coordinates match the panel's rotation (`MADCTL` MV/MX/MY bits).
5. **Clock & idle cadence**: Verify that `app.tick()` returns `false` when no input or animation is active, allowing your main loop to sleep (`wfi` or RTOS semaphore wait with timeout) until the next touch interrupt or timer deadline.
