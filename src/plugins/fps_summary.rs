use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::plugin::Plugin;
use crate::surface::Surface;

/// Periodic console summary of `FrameTimings` averages every N frames.
/// Reads `FrameTimings` and `FrameStats` from `World` so its numbers
/// match `EspPerfSummaryPlugin` and any other consumer driven from the
/// same `MonoClock`. Without a clock plugin installed, all timings
/// stay zero and the line is harmless noise.
///
/// **Inserts**
/// - resource: none (reads `FrameTimings` / `FrameStats` written by `App::run`)
/// - system: none
/// - view: none
/// - entity: none
/// - hooks: `post_render` (per-frame counter + periodic console summary)
pub struct FpsSummaryPlugin {
    frames_per_summary: u32,
    frame_count: u32,
    frame_ns_total: u64,
    render_ns_total: u64,
    on_summary: fn(report: FpsSummary<'_>),
}

/// Snapshot handed to the [`FpsSummaryPlugin`] sink each window.
/// Carries the windowed averages plus the `FrameStats` view (avg / min
/// / max / p99 / jitter over the last 256 frames).
pub struct FpsSummary<'a> {
    pub frames: u32,
    pub avg_frame_ns: u64,
    pub avg_render_ns: u64,
    pub stats: Option<&'a crate::ecs::FrameStats>,
}

impl FpsSummaryPlugin {
    pub fn new(frames_per_summary: u32) -> Self {
        Self {
            frames_per_summary,
            frame_count: 0,
            frame_ns_total: 0,
            render_ns_total: 0,
            on_summary: default_summary,
        }
    }

    /// Override where the summary line goes — log file, LCD overlay, etc.
    pub fn with_sink(mut self, sink: fn(FpsSummary<'_>)) -> Self {
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
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, world: &mut World, render_nanos: u64) {
        self.frame_count += 1;
        self.render_ns_total += render_nanos;
        if let Some(t) = world.resource::<crate::ecs::FrameTimings>() {
            self.frame_ns_total += t.frame_nanos;
        }
        if self.frame_count >= self.frames_per_summary {
            let avg_frame_ns = self.frame_ns_total / self.frame_count as u64;
            let avg_render_ns = self.render_ns_total / self.frame_count as u64;
            (self.on_summary)(FpsSummary {
                frames: self.frame_count,
                avg_frame_ns,
                avg_render_ns,
                stats: world.resource::<crate::ecs::FrameStats>(),
            });
            self.frame_count = 0;
            self.frame_ns_total = 0;
            self.render_ns_total = 0;
        }
    }
}

#[cfg(feature = "std")]
fn default_summary(report: FpsSummary<'_>) {
    let fps = if report.avg_frame_ns == 0 {
        0.0
    } else {
        1_000_000_000.0 / report.avg_frame_ns as f64
    };
    eprint!(
        "[fps] {} frames | frame {:.1}us ({:.1} fps) | render {:.1}us",
        report.frames,
        report.avg_frame_ns as f64 / 1000.0,
        fps,
        report.avg_render_ns as f64 / 1000.0,
    );
    if let Some(s) = report.stats {
        eprintln!(
            " | window={} min {}us max {}us p99 {}us jitter {}us",
            s.len(),
            s.min() / 1000,
            s.max() / 1000,
            s.p99() / 1000,
            s.jitter() / 1000,
        );
    } else {
        eprintln!();
    }
}

#[cfg(not(feature = "std"))]
fn default_summary(_report: FpsSummary<'_>) {
    // No-op on bare metal — users override with_sink to route to an overlay
    // or log UART.
}
