use alloc::vec::Vec;

use super::World;

/// Lower `priority` runs first; see [`run_order`] for named slots.
///
/// `last_us` / `total_us` / `call_count` are populated by
/// `SystemScheduler::run_all` when a `MonoClock` resource is present.
/// Without a clock all three stay zero and the scheduler doesn't pay
/// the timing overhead.
pub struct System {
    pub name: &'static str,
    pub priority: i32,
    pub run: fn(&mut World),
    pub last_us: u32,
    pub total_us: u64,
    pub call_count: u32,
}

impl System {
    pub const fn new(name: &'static str, priority: i32, run: fn(&mut World)) -> Self {
        Self {
            name,
            priority,
            run,
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }
}

/// Lower runs earlier. Spacing leaves room for user systems to slot
/// between built-ins; reuse a named slot when role matches.
pub mod run_order {
    /// Synthetic input emitters (sim, replay) — produce events
    /// downstream dispatch consumes this frame.
    pub const SIM_INPUT: i32 = 50;

    /// Clock / `DeltaTimeMs` sync. Anything reading `dt` runs after.
    pub const DELTA_TIME: i32 = 60;

    /// hover_system / press_system — read PointerCursor + hit_test, write
    /// InteractionState. After sim_input emitters but before animation.
    pub const INTERACTION_STATE: i32 = 80;

    /// Animation tickers that consume `dt`.
    pub const ANIMATION: i32 = 150;

    /// Declarative timers — same band as animation.
    pub const TIMER: i32 = 150;

    /// Inertia spring tick. Runs after pointer events, before
    /// `ScrollOffset` observers.
    pub const SCROLL_INERTIA: i32 = 250;

    /// Re-bind/reposition pool slots from current `ScrollOffset`.
    pub const LAZY_LIST: i32 = 350;

    /// TabBar.selected → page visibility.
    pub const TAB_PAGES: i32 = 350;

    /// Default for user systems — observes a fully-settled state.
    pub const NORMAL: i32 = 500;
}

#[derive(Default)]
pub struct SystemScheduler {
    systems: Vec<System>,
}

impl SystemScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stable on equal priority — registration order breaks ties.
    pub fn add(&mut self, system: System) {
        let pos = self
            .systems
            .iter()
            .position(|s| s.priority > system.priority)
            .unwrap_or(self.systems.len());
        self.systems.insert(pos, system);
    }

    pub fn run_all(&mut self, world: &mut World) {
        let clock_fn = world.resource::<super::MonoClock>().map(|c| c.clock);
        for system in &mut self.systems {
            let start_ns = clock_fn.map(|f| f()).unwrap_or(0);
            (system.run)(world);
            if let Some(f) = clock_fn {
                let elapsed_ns = f().saturating_sub(start_ns);
                let elapsed_us = (elapsed_ns / 1_000) as u32;
                system.last_us = elapsed_us;
                system.total_us = system.total_us.saturating_add(elapsed_us as u64);
                system.call_count = system.call_count.saturating_add(1);
            }
        }
    }

    pub fn reset_perf(&mut self) {
        for system in &mut self.systems {
            system.last_us = 0;
            system.total_us = 0;
            system.call_count = 0;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &System> {
        self.systems.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(_: &mut World) {}

    #[test]
    fn run_order_orders_ascending() {
        let mut s = SystemScheduler::new();
        s.add(System::new("late", 300, dummy));
        s.add(System::new("early", 100, dummy));
        s.add(System::new("mid", 200, dummy));
        let names: Vec<&str> = s.iter().map(|s| s.name).collect();
        assert_eq!(names, ["early", "mid", "late"]);
    }

    #[test]
    fn equal_run_order_preserves_insertion_order() {
        let mut s = SystemScheduler::new();
        s.add(System::new("a", 200, dummy));
        s.add(System::new("b", 200, dummy));
        s.add(System::new("c", 100, dummy));
        let names: Vec<&str> = s.iter().map(|s| s.name).collect();
        assert_eq!(names, ["c", "a", "b"]);
    }
}
