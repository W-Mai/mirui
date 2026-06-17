use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::MonoClock;
use crate::surface::Surface;

/// Sleeps in `post_render` to cap `App::run` at a target FPS. Needed
/// on every backend whose vsync runs inside `present`/`flush` —
/// mirui's dirty-aware loop skips those calls on idle frames so the
/// vsync wait never happens and the tick loop spins at 60 000+ fps.
/// `requestAnimationFrame`-driven backends (web canvas) are the
/// exception: rAF paces ticks externally and the cap is redundant.
///
/// Time comes from the `MonoClock` resource, so a clock plugin must
/// be installed first (e.g. `StdInstantClockPlugin` or an
/// MCU-specific one); without it the cap is a no-op.
///
/// **Inserts**
/// - hooks: `post_render`
pub struct FrameRateCapPlugin {
    period_ns: u64,
    last_frame_ns: Option<u64>,
    sleep_ns: fn(u64),
}

impl FrameRateCapPlugin {
    /// `target_fps` of 0 panics. On `std` prefer [`Self::new`]; on
    /// `no_std` pass an MCU blocking delay (`embassy::time::Timer`,
    /// `Ets::delay_us`, FreeRTOS `vTaskDelay`).
    pub fn with_sleep_fn(target_fps: u32, sleep_ns: fn(u64)) -> Self {
        assert!(target_fps > 0, "FrameRateCapPlugin: target_fps must be > 0");
        Self {
            period_ns: 1_000_000_000 / u64::from(target_fps),
            last_frame_ns: None,
            sleep_ns,
        }
    }

    #[cfg(feature = "std")]
    pub fn new(target_fps: u32) -> Self {
        fn std_sleep(ns: u64) {
            std::thread::sleep(core::time::Duration::from_nanos(ns));
        }
        Self::with_sleep_fn(target_fps, std_sleep)
    }
}

impl<B, F> Plugin<B, F> for FrameRateCapPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, world: &mut crate::ecs::World, _render_nanos: u64) {
        let Some(now_ns) = world.resource::<MonoClock>().map(|c| c.now_ns()) else {
            return;
        };
        let target = self
            .last_frame_ns
            .map_or(now_ns, |prev| prev + self.period_ns);
        if target > now_ns {
            (self.sleep_ns)(target - now_ns);
        }
        // Re-anchor to `now_ns` if we missed the target — otherwise
        // a long stall (backgrounded window, vsync hang) leaves a
        // deficit that suppresses sleeps for hundreds of frames.
        self.last_frame_ns = Some(target.max(now_ns));
    }
}
