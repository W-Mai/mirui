use crate::app::{App, RendererFactory};
use crate::backend::Backend;
use crate::ecs::World;
use crate::plugin::Plugin;

/// Accumulates render timings and prints (via `println!` on std) every N frames.
/// With no clock plugin installed `render_nanos` is 0 and the average stays 0 —
/// still harmless, still shows frame count.
pub struct FpsSummaryPlugin {
    frames_per_summary: u32,
    frame_count: u32,
    render_ns_total: u64,
    on_summary: fn(frames: u32, avg_render_ns: u64),
}

impl FpsSummaryPlugin {
    pub fn new(frames_per_summary: u32) -> Self {
        Self {
            frames_per_summary,
            frame_count: 0,
            render_ns_total: 0,
            on_summary: default_summary,
        }
    }

    /// Override where the summary line goes — log file, LCD overlay, etc.
    pub fn with_sink(mut self, sink: fn(u32, u64)) -> Self {
        self.on_summary = sink;
        self
    }
}

impl Default for FpsSummaryPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B, F> Plugin<B, F> for FpsSummaryPlugin
where
    B: Backend,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, _world: &mut World, render_nanos: u64) {
        self.frame_count += 1;
        self.render_ns_total += render_nanos;
        if self.frame_count >= self.frames_per_summary {
            let avg = if self.frame_count == 0 {
                0
            } else {
                self.render_ns_total / self.frame_count as u64
            };
            (self.on_summary)(self.frame_count, avg);
            self.frame_count = 0;
            self.render_ns_total = 0;
        }
    }
}

#[cfg(feature = "std")]
fn default_summary(frames: u32, avg_render_ns: u64) {
    eprintln!(
        "[fps] {frames} frames, avg render: {:.1} µs",
        avg_render_ns as f64 / 1000.0
    );
}

#[cfg(not(feature = "std"))]
fn default_summary(_frames: u32, _avg_render_ns: u64) {
    // No-op on bare metal — users override with_sink to route to an overlay
    // or log UART.
}
