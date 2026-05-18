use alloc::vec::Vec;

use super::World;

/// Lower `priority` runs first; see [`run_order`] for named slots.
pub struct System {
    pub name: &'static str,
    pub priority: i32,
    pub run: fn(&mut World),
}

impl System {
    pub const fn new(name: &'static str, priority: i32, run: fn(&mut World)) -> Self {
        Self {
            name,
            priority,
            run,
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
        for system in &self.systems {
            (system.run)(world);
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
