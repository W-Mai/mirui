//! Wraps another `Surface` and sleeps inside `flush` to approximate
//! slow embedded displays (SPI / parallel RGB) when profiling on
//! host. `std` only — bare metal has no `thread::sleep`.

use super::{BackbufferPersistence, DisplayInfo, FramebufferAccess, InputEvent, Surface};
use crate::draw::texture::Texture;
use crate::types::Rect;
use core::time::Duration;

/// Approximate cost of a 16-bit pixel pushed over 80 MHz SPI:
/// ~200 ns/px ≈ 32 ms for a 240×240 frame. Pick a smaller value for
/// faster wall-clock iteration when profiling.
pub const NS_PER_PIXEL_SPI_80MHZ_RGB565: u32 = 200;

pub struct SlowSurface<S: Surface> {
    inner: S,
    ns_per_pixel: u32,
}

impl<S: Surface> SlowSurface<S> {
    pub fn new(inner: S, ns_per_pixel: u32) -> Self {
        Self {
            inner,
            ns_per_pixel,
        }
    }

    pub fn ns_per_pixel(&self) -> u32 {
        self.ns_per_pixel
    }

    pub fn set_ns_per_pixel(&mut self, ns: u32) {
        self.ns_per_pixel = ns;
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: Surface> crate::cache::InspectCaches for SlowSurface<S> {
    fn inspect_caches(
        &self,
    ) -> impl Iterator<Item = (&'static str, &dyn crate::cache::CacheInspect)> + '_ {
        self.inner.inspect_caches()
    }
}

impl<S: Surface> Surface for SlowSurface<S> {
    fn display_info(&self) -> DisplayInfo {
        self.inner.display_info()
    }

    fn flush(&mut self, area: &Rect) {
        // `area` is in physical pixels (`App::render_dirty` did the
        // logical→physical conversion), so w*h matches the byte
        // stream size the real backend would push.
        let w = area.w.to_int().max(0) as u64;
        let h = area.h.to_int().max(0) as u64;
        let pixel_count = w.saturating_mul(h);
        let sleep_ns = pixel_count.saturating_mul(self.ns_per_pixel as u64);
        if sleep_ns > 0 {
            std::thread::sleep(Duration::from_nanos(sleep_ns));
        }
        self.inner.flush(area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.inner.poll_event()
    }

    fn screen_rect(&self) -> Rect {
        self.inner.screen_rect()
    }

    fn physical_size(&self) -> (u32, u32) {
        self.inner.physical_size()
    }

    fn persistence(&self) -> BackbufferPersistence {
        self.inner.persistence()
    }
}

impl<S: FramebufferAccess> FramebufferAccess for SlowSurface<S> {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.inner.framebuffer()
    }
}
