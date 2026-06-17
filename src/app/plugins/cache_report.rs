use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::core::cache::{CacheRegistry, CacheStatsSnapshot};
use crate::ecs::World;
use crate::surface::Surface;

/// Calls a sink with the latest [`CacheRegistry`] snapshot every N
/// frames so user code can observe live backend cache stats (entries,
/// hit, miss, max_size) without poking the backend directly.
///
/// **Inserts**
/// - hooks: `post_render`
pub struct CacheReportPlugin {
    frames_per_report: u32,
    frame_count: u32,
    on_report: fn(report: CacheReport<'_>),
}

/// Snapshot handed to the [`CacheReportPlugin`] sink each window.
pub struct CacheReport<'a> {
    pub frames: u32,
    pub snapshots: &'a [CacheStatsSnapshot],
}

impl CacheReportPlugin {
    pub fn new(frames_per_report: u32) -> Self {
        Self {
            frames_per_report,
            frame_count: 0,
            on_report: default_report,
        }
    }

    pub fn with_sink(mut self, sink: fn(CacheReport<'_>)) -> Self {
        self.on_report = sink;
        self
    }
}

impl Default for CacheReportPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B, F> Plugin<B, F> for CacheReportPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        self.frame_count += 1;
        if self.frame_count < self.frames_per_report {
            return;
        }
        let Some(reg) = world.resource::<CacheRegistry>() else {
            self.frame_count = 0;
            return;
        };
        (self.on_report)(CacheReport {
            frames: self.frame_count,
            snapshots: reg.snapshots(),
        });
        self.frame_count = 0;
    }
}

#[cfg(feature = "std")]
fn default_report(report: CacheReport<'_>) {
    if report.snapshots.is_empty() {
        return;
    }
    eprintln!("[cache] after {} frames", report.frames);
    for snap in report.snapshots {
        let total = snap.stats.hit_count + snap.stats.miss_count;
        let hit_rate = if total == 0 {
            0.0
        } else {
            snap.stats.hit_count as f64 * 100.0 / total as f64
        };
        eprintln!(
            "  {} entries={} hit={} miss={} ({:.1}%) max={:?}",
            snap.name,
            snap.len,
            snap.stats.hit_count,
            snap.stats.miss_count,
            hit_rate,
            snap.max_size,
        );
    }
}

#[cfg(not(feature = "std"))]
fn default_report(_report: CacheReport<'_>) {}
