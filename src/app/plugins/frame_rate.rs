use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::MonoClock;
use crate::surface::Surface;

/// Caps logical ticks, including idle and failed drawing attempts. Blocking hosts sleep after surface maintenance; mobile hosts use the same deadline to schedule updates without blocking window events. Web animation frames already pace ticks externally.
///
/// Time comes from the `MonoClock` resource, so a clock plugin must
/// be installed first (e.g. `StdInstantClockPlugin` or an
/// MCU-specific one); without it the cap is a no-op.
///
/// **Inserts**
/// - resource: private tick pacing state
pub struct FrameRateCapPlugin {
    period_ns: u64,
    sleep_ns: fn(u64),
}

pub(crate) struct FramePacing {
    period_ns: u64,
    next_tick_ns: Option<u64>,
    sleep_ns: fn(u64),
}

impl FramePacing {
    fn remaining(&self, now_ns: u64) -> u64 {
        self.next_tick_ns.unwrap_or(now_ns).saturating_sub(now_ns)
    }

    fn complete(&mut self, now_ns: u64, blocking: bool) {
        let target = self.next_tick_ns.unwrap_or(now_ns);
        let wait = self.remaining(now_ns);
        if blocking && wait > 0 {
            (self.sleep_ns)(wait);
        }
        self.next_tick_ns = Some(target.max(now_ns).saturating_add(self.period_ns));
    }
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn remaining(world: &crate::ecs::World) -> Option<u64> {
    let now = world.resource::<MonoClock>()?.now_ns();
    Some(world.resource::<FramePacing>()?.remaining(now))
}

pub(crate) fn complete(world: &mut crate::ecs::World, blocking: bool) {
    let Some(now) = world.resource::<MonoClock>().map(MonoClock::now_ns) else {
        return;
    };
    if let Some(pacing) = world.resource_mut::<FramePacing>() {
        pacing.complete(now, blocking);
    }
}

impl FrameRateCapPlugin {
    /// `target_fps` of 0 panics. On `std` prefer [`Self::new`]; on
    /// `no_std` pass an MCU blocking delay (`embassy::time::Timer`,
    /// `Ets::delay_us`, FreeRTOS `vTaskDelay`).
    pub fn with_sleep_fn(target_fps: u32, sleep_ns: fn(u64)) -> Self {
        assert!(target_fps > 0, "FrameRateCapPlugin: target_fps must be > 0");
        Self {
            period_ns: (1_000_000_000 / u64::from(target_fps)).max(1),
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
    fn build(&mut self, app: &mut App<B, F>) {
        app.world.insert_resource(FramePacing {
            period_ns: self.period_ns,
            next_tick_ns: None,
            sleep_ns: self.sleep_ns,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    std::thread_local! { static SLEPT: Cell<u64> = const { Cell::new(0) }; }

    #[test]
    fn blocking_and_host_scheduling_share_deadlines_and_reanchor_after_stalls() {
        let mut pacing = FramePacing {
            period_ns: 10,
            next_tick_ns: None,
            sleep_ns: |ns| SLEPT.set(SLEPT.get() + ns),
        };
        assert_eq!(pacing.remaining(100), 0);
        pacing.complete(100, true);
        assert_eq!(SLEPT.get(), 0);
        assert_eq!(pacing.remaining(104), 6);
        pacing.complete(104, true);
        assert_eq!(SLEPT.get(), 6);
        assert_eq!(pacing.remaining(112), 8);
        pacing.complete(500, false);
        assert_eq!(pacing.remaining(500), 10);
        assert_eq!(SLEPT.get(), 6);
        pacing.complete(510, false);
        assert_eq!(pacing.remaining(511), 9);
    }
}
