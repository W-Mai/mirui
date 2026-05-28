use alloc::vec::Vec;
use core::any::TypeId;

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
    /// When non-empty, the scheduler skips this system's run if no
    /// live entity owns *any* of these component types. Each entry
    /// is a `fn() -> TypeId` so the slice is constructible in const
    /// context on stable Rust (`TypeId::of` is only `const` ≥ 1.85).
    pub expect: &'static [fn() -> TypeId],
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
            expect: &[],
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }

    /// Pass an empty slice to clear the expect tag (the default).
    pub const fn with_expect(mut self, expect: &'static [fn() -> TypeId]) -> Self {
        self.expect = expect;
        self
    }
}

/// Named slots for `System::priority`. Lower runs earlier.
///
/// Spacing leaves room for user systems to slot between built-ins;
/// reuse a named slot when the role matches a documented one rather
/// than picking a fresh integer.
///
/// Two ways to reference a slot in `#[mirui::system(order = ...)]`:
///
/// - `order = SystemSlot::Normal` — preferred; type-checked, IDE-completable
/// - `order = NORMAL` — bare constant from `run_order` module; backward-compatible
///
/// New code should reach for the enum; the bare constants stay because
/// the proc-macro glob-imports them today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemSlot {
    /// Synthetic input emitters (sim, replay).
    SimInput,
    /// Clock / `DeltaTimeMs` sync. Anything reading `dt` runs after.
    DeltaTime,
    /// hover_system / press_system — read PointerCursor + hit_test, write `InteractionState`.
    InteractionState,
    /// Animation tickers that consume `dt`.
    Animation,
    /// Declarative timers — same band as animation.
    Timer,
    /// Inertia spring tick. Runs after pointer events, before `ScrollOffset` observers.
    ScrollInertia,
    /// Re-bind/reposition pool slots from current `ScrollOffset`.
    LazyList,
    /// `TabBar.selected` → page visibility.
    TabPages,
    /// Default for user systems — observes fully-settled state.
    Normal,
    /// Final reconciliation just before render (e.g. consumer dirty
    /// propagation that must see all user-system writes).
    PreRender,
}

impl SystemSlot {
    /// Numeric priority for `System::new`. Lower runs earlier.
    pub const fn priority(self) -> i32 {
        match self {
            Self::SimInput => 50,
            Self::DeltaTime => 60,
            Self::InteractionState => 80,
            Self::Animation => 150,
            Self::Timer => 150,
            Self::ScrollInertia => 250,
            Self::LazyList => 350,
            Self::TabPages => 350,
            Self::Normal => 500,
            Self::PreRender => 700,
        }
    }
}

impl From<SystemSlot> for i32 {
    fn from(slot: SystemSlot) -> i32 {
        slot.priority()
    }
}

/// Bare-constant aliases for [`SystemSlot::priority`]. Kept because the
/// `#[mirui::system]` proc-macro glob-imports this module to support
/// `order = NORMAL`-style attribute syntax.
pub mod run_order {
    use super::SystemSlot;
    pub const SIM_INPUT: i32 = SystemSlot::SimInput.priority();
    pub const DELTA_TIME: i32 = SystemSlot::DeltaTime.priority();
    pub const INTERACTION_STATE: i32 = SystemSlot::InteractionState.priority();
    pub const ANIMATION: i32 = SystemSlot::Animation.priority();
    pub const TIMER: i32 = SystemSlot::Timer.priority();
    pub const SCROLL_INERTIA: i32 = SystemSlot::ScrollInertia.priority();
    pub const LAZY_LIST: i32 = SystemSlot::LazyList.priority();
    pub const TAB_PAGES: i32 = SystemSlot::TabPages.priority();
    pub const NORMAL: i32 = SystemSlot::Normal.priority();
    pub const PRE_RENDER: i32 = SystemSlot::PreRender.priority();
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
            if !system.expect.is_empty()
                && !system
                    .expect
                    .iter()
                    .any(|tid_fn| world.has_any_by_id(tid_fn()))
            {
                system.last_us = 0;
                continue;
            }
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
    fn expect_skips_system_when_no_entity_has_component() {
        struct Marker;
        struct Counter(u32);

        fn ticking(world: &mut World) {
            if let Some(c) = world.resource_mut::<Counter>() {
                c.0 += 1;
            }
        }

        let mut world = World::new();
        world.insert_resource(Counter(0));

        let mut s = SystemScheduler::new();
        s.add(System::new("ticking", 100, ticking).with_expect(&[TypeId::of::<Marker>]));

        // No entity carries `Marker` → system must not run.
        s.run_all(&mut world);
        assert_eq!(world.resource::<Counter>().unwrap().0, 0);

        // Spawn an entity with `Marker` → system runs.
        let e = world.spawn();
        world.insert(e, Marker);
        s.run_all(&mut world);
        assert_eq!(world.resource::<Counter>().unwrap().0, 1);

        // Despawn it → system stops running again.
        world.despawn(e);
        s.run_all(&mut world);
        assert_eq!(world.resource::<Counter>().unwrap().0, 1);
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

    #[test]
    fn system_slot_priorities_match_run_order_constants() {
        assert_eq!(SystemSlot::SimInput.priority(), run_order::SIM_INPUT);
        assert_eq!(SystemSlot::DeltaTime.priority(), run_order::DELTA_TIME);
        assert_eq!(
            SystemSlot::InteractionState.priority(),
            run_order::INTERACTION_STATE
        );
        assert_eq!(SystemSlot::Animation.priority(), run_order::ANIMATION);
        assert_eq!(SystemSlot::Timer.priority(), run_order::TIMER);
        assert_eq!(
            SystemSlot::ScrollInertia.priority(),
            run_order::SCROLL_INERTIA
        );
        assert_eq!(SystemSlot::LazyList.priority(), run_order::LAZY_LIST);
        assert_eq!(SystemSlot::TabPages.priority(), run_order::TAB_PAGES);
        assert_eq!(SystemSlot::Normal.priority(), run_order::NORMAL);
        assert_eq!(SystemSlot::PreRender.priority(), run_order::PRE_RENDER);
    }

    #[test]
    fn system_slot_into_i32_returns_priority() {
        let p: i32 = SystemSlot::Animation.into();
        assert_eq!(p, 150);
    }

    /// Architecture invariant: every plugin in src/plugins/*.rs must
    /// have a `**Inserts**` section in a docstring that survives into
    /// the source file (i.e. the docstring directly above the struct).
    /// This is the v0.17.1 plugin documentation contract; if a plugin
    /// adds resources / systems / views silently, this test reminds the
    /// author to document them.
    #[test]
    fn plugins_declare_inserts_section() {
        let plugins = [
            ("perf_report", include_str!("../plugins/perf_report.rs")),
            ("fps_summary", include_str!("../plugins/fps_summary.rs")),
            (
                "input_feedback",
                include_str!("../plugins/input_feedback.rs"),
            ),
            #[cfg(feature = "std")]
            ("std_clock", include_str!("../plugins/std_clock.rs")),
        ];
        for (name, src) in plugins {
            assert!(
                src.contains("**Inserts**"),
                "plugin {name} missing **Inserts** doc section",
            );
        }
    }
}
