use alloc::vec::Vec;

use crate::anim::EaseFn;
use crate::ecs::{Entity, World};
use crate::types::Fixed;

use super::gesture::GestureSystem;
use super::hit_test::hit_test;
use super::input::InputEvent;

#[derive(Clone, Copy, Debug)]
pub enum SimTiming {
    At(u32),
    After(u32),
}

#[derive(Clone, Debug)]
pub struct SimCommand {
    pub timing: SimTiming,
    pub event: InputEvent,
}

impl SimCommand {
    pub fn at(ms: u32, event: InputEvent) -> Self {
        Self {
            timing: SimTiming::At(ms),
            event,
        }
    }

    pub fn after(ms: u32, event: InputEvent) -> Self {
        Self {
            timing: SimTiming::After(ms),
            event,
        }
    }
}

pub struct SimulatedInput {
    commands: Vec<SimCommand>,
    resolved_ms: Vec<u32>,
    cursor: usize,
    start_ms: Option<u32>,
    looping: bool,
    pub root: Option<Entity>,
}

impl SimulatedInput {
    pub fn new(commands: Vec<SimCommand>) -> Self {
        let resolved = resolve_timings(&commands);
        Self {
            commands,
            resolved_ms: resolved,
            cursor: 0,
            start_ms: None,
            looping: false,
            root: None,
        }
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    pub fn with_root(mut self, root: Entity) -> Self {
        self.root = Some(root);
        self
    }

    pub fn total_duration_ms(&self) -> u32 {
        self.resolved_ms.last().copied().unwrap_or(0)
    }
}

fn resolve_timings(commands: &[SimCommand]) -> Vec<u32> {
    let mut resolved = Vec::with_capacity(commands.len());
    let mut current_ms: u32 = 0;
    for cmd in commands {
        match cmd.timing {
            SimTiming::At(ms) => current_ms = ms,
            SimTiming::After(ms) => current_ms = current_ms.saturating_add(ms),
        }
        resolved.push(current_ms);
    }
    resolved
}

pub fn sim_input_system(world: &mut World) {
    let clock_fn = world.resource::<crate::ecs::MonoClock>().map(|fc| fc.clock);
    let now_ms = clock_fn.map(|f| (f() / 1_000_000) as u32).unwrap_or(0);

    let (commands_to_fire, root, screen_w, screen_h) = {
        let Some(sim) = world.resource_mut::<SimulatedInput>() else {
            return;
        };
        if sim.commands.is_empty() {
            return;
        }

        let start = *sim.start_ms.get_or_insert(now_ms);
        let elapsed = now_ms.wrapping_sub(start);

        let mut fired: Vec<(InputEvent, bool)> = Vec::new();
        while sim.cursor < sim.commands.len() {
            let target_ms = sim.resolved_ms[sim.cursor];
            if elapsed >= target_ms {
                let is_down = matches!(
                    sim.commands[sim.cursor].event,
                    InputEvent::PointerDown { .. }
                );
                fired.push((sim.commands[sim.cursor].event.clone(), is_down));
                sim.cursor += 1;
            } else {
                break;
            }
        }

        if sim.cursor >= sim.commands.len() && sim.looping {
            sim.cursor = 0;
            sim.start_ms = Some(now_ms);
        }

        let root = sim.root;
        let (sw, sh) = world
            .resource::<crate::surface::DisplayInfo>()
            .map(|d| (d.width, d.height))
            .unwrap_or((128, 128));
        (fired, root, sw, sh)
    };

    let root = match root {
        Some(r) => r,
        None => match world.resource::<SimRootFallback>() {
            Some(f) => f.0,
            None => return,
        },
    };

    for (event, is_down) in &commands_to_fire {
        let hit = if *is_down {
            match event {
                InputEvent::PointerDown { x, y, .. } => {
                    hit_test(world, root, *x, *y, screen_w, screen_h)
                }
                _ => None,
            }
        } else {
            None
        };

        let now_ms_inner = clock_fn.map(|f| (f() / 1_000_000) as u32).unwrap_or(0);

        if let Some(gs) = world.resource_mut::<GestureSystem>() {
            gs.recognizer
                .update(event, now_ms_inner, hit, &mut gs.events);
        }
    }

    let pending: Vec<super::gesture::GestureEvent> = world
        .resource_mut::<GestureSystem>()
        .map(|gs| gs.events.drain().collect())
        .unwrap_or_default();
    for gesture in &pending {
        super::bubble_dispatch(world, gesture);
    }
}

struct SimRootFallback(Entity);

/// Convenience: if the user doesn't set `SimulatedInput.root`, the system
/// reads from App's root. Call this once after `set_root` to stash it.
pub fn set_sim_root(world: &mut World, root: Entity) {
    world.insert_resource(SimRootFallback(root));
}

// ─── High-level timeline API ───────────────────────────────────────────

use crate::types::Point;

#[derive(Clone, Copy)]
pub enum SimAction {
    Tap(Point),
    Drag {
        from: Point,
        to: Point,
        duration_ms: u16,
        ease: EaseFn,
    },
    Wait(u32),
}

#[derive(Clone, Copy)]
struct TimelineEntry {
    action: SimAction,
    start_ms: u32,
}

pub struct SimTimeline {
    entries: Vec<TimelineEntry>,
    cursor: usize,
    action_elapsed_ms: u32,
    action_started: bool,
    start_ms: Option<u32>,
    looping: bool,
    pub total_ms: u32,
}

impl SimTimeline {
    pub fn new(actions: Vec<SimAction>) -> Self {
        let mut entries = Vec::with_capacity(actions.len());
        let mut t: u32 = 0;
        for action in &actions {
            entries.push(TimelineEntry {
                action: *action,
                start_ms: t,
            });
            t += match action {
                SimAction::Tap(_) => 100,
                SimAction::Drag { duration_ms, .. } => *duration_ms as u32,
                SimAction::Wait(ms) => *ms,
            };
        }
        Self {
            entries,
            cursor: 0,
            action_elapsed_ms: 0,
            action_started: false,
            start_ms: None,
            looping: false,
            total_ms: t,
        }
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }
}

pub fn sim_timeline_system(world: &mut World) {
    let clock_fn = world.resource::<crate::ecs::MonoClock>().map(|fc| fc.clock);
    let now_ms = clock_fn.map(|f| (f() / 1_000_000) as u32).unwrap_or(0);

    let (lw, lh) = world
        .resource::<crate::surface::DisplayInfo>()
        .map(|d| (d.width, d.height))
        .unwrap_or((128, 128));
    let root = world
        .resource::<SimRootFallback>()
        .map(|f| f.0)
        .unwrap_or(Entity {
            id: 0,
            generation: 0,
        });

    let Some(tl) = world.resource_mut::<SimTimeline>() else {
        return;
    };
    if tl.entries.is_empty() {
        return;
    }

    let start = *tl.start_ms.get_or_insert(now_ms);
    let elapsed = now_ms.wrapping_sub(start);

    if tl.cursor >= tl.entries.len() {
        if tl.looping {
            tl.cursor = 0;
            tl.action_started = false;
            tl.start_ms = Some(now_ms);
        }
        return;
    }

    let entry = tl.entries[tl.cursor];
    if elapsed < entry.start_ms {
        return;
    }

    let action_elapsed = elapsed - entry.start_ms;

    match entry.action {
        SimAction::Tap(pt) => {
            if !tl.action_started {
                tl.action_started = true;
                tl.action_elapsed_ms = 0;
                let event = InputEvent::PointerDown {
                    id: 0,
                    x: pt.x,
                    y: pt.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= 50 {
                let event = InputEvent::PointerUp {
                    id: 0,
                    x: pt.x,
                    y: pt.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                }
            }
        }
        SimAction::Drag {
            from,
            to,
            duration_ms,
            ease,
        } => {
            if !tl.action_started {
                tl.action_started = true;
                tl.action_elapsed_ms = 0;
                let event = InputEvent::PointerDown {
                    id: 0,
                    x: from.x,
                    y: from.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= duration_ms as u32 {
                let event = InputEvent::PointerUp {
                    id: 0,
                    x: to.x,
                    y: to.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                }
            } else {
                let t = Fixed::from_raw(
                    (action_elapsed as i32) * Fixed::ONE.raw() / (duration_ms as i32),
                );
                let eased = ease(t);
                let x = from.x + eased * (to.x - from.x);
                let y = from.y + eased * (to.y - from.y);
                let event = InputEvent::PointerMove { id: 0, x, y };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            }
        }
        SimAction::Wait(ms) => {
            if action_elapsed >= ms {
                tl.cursor += 1;
                tl.action_started = false;
            }
        }
    }

    let pending: Vec<super::gesture::GestureEvent> = world
        .resource_mut::<GestureSystem>()
        .map(|gs| gs.events.drain().collect())
        .unwrap_or_default();
    for gesture in &pending {
        super::bubble_dispatch(world, gesture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::time::{MonoClock, mock};
    use crate::types::Point;

    fn setup_world() -> World {
        let mut world = World::default();
        world.insert_resource(crate::event::scroll::ScrollDragState::default());
        world.insert_resource(crate::event::scroll::ScrollSpring::default());
        world.insert_resource(GestureSystem::default());
        world.insert_resource(crate::event::focus::FocusState::default());
        world.insert_resource(crate::surface::DisplayInfo {
            width: 128,
            height: 128,
            scale: crate::types::Fixed::ONE,
            format: crate::draw::texture::ColorFormat::RGBA8888,
        });
        let root = world.spawn();
        world.insert_resource(SimRootFallback(root));
        // Caller is expected to hold mock::lock() for the test's
        // duration so the global mock clock isn't shared with parallel
        // tests.
        mock::set_ns(0);
        world.insert_resource(MonoClock::new(mock::clock_fn));
        world
    }

    #[test]
    fn wait_holds_cursor_for_full_duration() {
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::Wait(800)]));

        sim_timeline_system(&mut world);
        assert_eq!(world.resource::<SimTimeline>().unwrap().cursor, 0);

        for _ in 0..25 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            0,
            "cursor advanced during Wait — timeline drifted",
        );

        for _ in 0..30 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "cursor failed to advance past Wait",
        );
    }

    /// Drive sim_timeline for `step_ms` per call, recording every
    /// cursor change with the wall-clock-equivalent ms at which it
    /// occurred. Returns the (cursor, ms) trace.
    fn trace_cursors(
        world: &mut World,
        step_ms: u64,
        max_steps: u32,
    ) -> alloc::vec::Vec<(usize, u64)> {
        let mut trace = alloc::vec::Vec::new();
        let mut elapsed: u64 = 0;
        let mut last: usize = usize::MAX;
        for _ in 0..max_steps {
            sim_timeline_system(world);
            let cur = world
                .resource::<SimTimeline>()
                .map(|t| t.cursor)
                .unwrap_or(usize::MAX);
            if cur != last {
                trace.push((cur, elapsed));
                last = cur;
            }
            mock::advance_ms(step_ms);
            elapsed += step_ms;
        }
        trace
    }

    #[test]
    fn wait_survives_u32_ms_wrap() {
        // sim_timeline computes `elapsed = now_ms.wrapping_sub(start)`.
        // When the clock is positioned so that within one Wait the
        // u32-ms representation wraps, `wrapping_sub` must still yield
        // the correct relative duration. Tests that the wrap doesn't
        // collapse a 800-ms Wait into an instant-fire (or stretch it
        // across the full u32 range).
        let _g = mock::lock();
        let mut world = setup_world();
        // Position the clock 200 ms before u32::MAX-ms wraps. A 800-ms
        // Wait will cross the boundary mid-way.
        mock::set_ms((u32::MAX as u64) - 200);
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::Wait(800)]));

        // Halfway through the Wait — cursor must still be 0.
        sim_timeline_system(&mut world);
        for _ in 0..25 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            0,
            "Wait collapsed across u32 ms wrap",
        );

        // Past full 800 ms — should advance.
        for _ in 0..30 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "Wait did not advance after full duration across wrap",
        );
    }

    #[test]
    fn cycle_stability_two_cycles_match() {
        // Wait 100, Tap (100), Wait 800, Tap (100), Wait 100. Total
        // 1200 ms per cycle. Looping. Verify cycle 2's per-cursor
        // timings match cycle 1's (no drift in re-run).
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(
            SimTimeline::new(alloc::vec![
                SimAction::Wait(100),
                SimAction::Tap(Point::new(10, 10)),
                SimAction::Wait(800),
                SimAction::Tap(Point::new(20, 20)),
                SimAction::Wait(100),
            ])
            .looping(true),
        );

        let step = 16u64;
        // Two full cycles + buffer = 2 × 1200 + 800 = 3200 ms ≈ 200
        // frames @ 16 ms/frame.
        let trace = trace_cursors(&mut world, step, 250);

        // Find the *second* time cursor returns to 0 (cycle 2 reset).
        // Cycle 1 reset is the first (after the timeline length is
        // exceeded). Total ms of cycle 1 = ms at second 0.
        let zero_resets: alloc::vec::Vec<u64> = trace
            .iter()
            .filter(|(c, _)| *c == 0)
            .map(|(_, ms)| *ms)
            .collect();

        // Initial state is cursor=0 at ms=0; first looping reset is
        // the second 0; second cycle's reset is the third 0.
        assert!(
            zero_resets.len() >= 3,
            "expected at least 3 cursor=0 events (initial + 2 loop resets), \
             trace={trace:?}",
        );

        let cycle1_len = zero_resets[1] - zero_resets[0];
        let cycle2_len = zero_resets[2] - zero_resets[1];
        let drift = cycle1_len.abs_diff(cycle2_len);

        assert!(
            drift <= step,
            "cycle drift {drift} ms > frame ({step} ms): \
             cycle1={cycle1_len} ms, cycle2={cycle2_len} ms\n\
             trace={trace:?}",
        );

        // Sanity: each cycle should be ~1200 ms.
        assert!(
            (1100..=1300).contains(&cycle1_len),
            "cycle1 length {cycle1_len} ms outside [1100, 1300]; trace={trace:?}",
        );
    }

    // -- Widget-level integration tests ---------------------------
    //
    // These build a tree resembling demo_widgets (TabBar + 3 pages,
    // Slider on page 1, Switch on page 2) and exercise the full
    // input pipeline through dispatch_input + sim_timeline. Their
    // job is to reproduce ESP-side behaviours (cycle drift,
    // hit_test mis-routing, lost toggles) deterministically on host.

    use crate::components::slider::Slider;
    use crate::components::switch::Switch;
    use crate::components::tab_pages::{TabContent, tab_pages_system};
    use crate::components::tabbar::TabBar;
    use crate::draw::texture::ColorFormat;
    use crate::event::dispatch_input;
    use crate::event::focus::FocusState;
    use crate::event::hit_test::hit_test;
    use crate::event::scroll::ScrollSpring;
    use crate::layout::{AlignItems, FlexDirection, JustifyContent, LayoutStyle};
    use crate::surface::{DisplayInfo, InputEvent};
    use crate::types::Dimension;
    use crate::widget::builder::WidgetBuilder;
    use crate::widget::view::install_default_registry;
    use crate::widget::{Children, Parent};

    fn build_widget_world() -> (World, Entity, Entity, Entity) {
        let mut world = World::default();
        install_default_registry(&mut world);
        world.insert_resource(crate::event::scroll::ScrollDragState::default());
        world.insert_resource(ScrollSpring::default());
        world.insert_resource(GestureSystem::default());
        world.insert_resource(FocusState::default());
        world.insert_resource(DisplayInfo {
            width: 128,
            height: 128,
            scale: Fixed::ONE,
            format: ColorFormat::RGBA8888,
        });

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(128),
                height: Dimension::px(128),
                ..Default::default()
            })
            .id();

        let tab_bar = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(128),
                height: Dimension::px(14),
                ..Default::default()
            })
            .id();
        world.insert(tab_bar, TabBar::new(3));
        world.insert(tab_bar, Parent(root));
        if let Some(rc) = world.get_mut::<Children>(root) {
            rc.0.push(tab_bar);
        }

        let make_page = |world: &mut World, idx: u8| -> Entity {
            let p = WidgetBuilder::new(world)
                .layout(LayoutStyle {
                    width: Dimension::px(128),
                    height: Dimension::px(114),
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    ..Default::default()
                })
                .id();
            world.insert(
                p,
                TabContent {
                    tab_bar,
                    index: idx,
                },
            );
            world.insert(p, Parent(root));
            if let Some(rc) = world.get_mut::<Children>(root) {
                rc.0.push(p);
            }
            p
        };

        let _list_page = make_page(&mut world, 0);

        let slide_page = make_page(&mut world, 1);
        let slider = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(108),
                height: Dimension::px(12),
                ..Default::default()
            })
            .id();
        world.insert(slider, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
        world.insert(slider, Parent(slide_page));
        if let Some(rc) = world.get_mut::<Children>(slide_page) {
            rc.0.push(slider);
        }

        let switch_page = make_page(&mut world, 2);
        let switch = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(26),
                ..Default::default()
            })
            .id();
        world.insert(switch, Switch::new());
        world.insert(switch, Parent(switch_page));
        if let Some(rc) = world.get_mut::<Children>(switch_page) {
            rc.0.push(switch);
        }

        crate::event::widget_input::attach_widget_input_handlers(&mut world, root);
        super::set_sim_root(&mut world, root);

        (world, root, slider, switch)
    }

    /// Bug repro for ESP cycle-2 toggle-Tap-misses-Switch:
    /// Hit-testing at the Switch's centre should return the Switch
    /// entity, not the TabContent wrapper that hosts it.
    #[test]
    fn hit_test_finds_switch_when_sw_tab_visible() {
        let _g = mock::lock();
        mock::set_ms(0);
        let (mut world, root, _slider, switch) = build_widget_world();
        world.insert_resource(MonoClock::new(mock::clock_fn));

        let bar = world.query::<TabBar>().collect();
        let bar = bar.first().copied().expect("TabBar entity");
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 2;
        }
        tab_pages_system(&mut world);

        let cx = Fixed::from_int(64);
        let cy = Fixed::from_int(71);
        let hit = hit_test(&world, root, cx, cy, 128, 128);
        assert_eq!(
            hit,
            Some(switch),
            "hit_test at Switch centre returned {:?}, expected Switch entity {:?}",
            hit,
            switch,
        );
    }

    /// Bug repro: a Tap event delivered through dispatch_input at
    /// the Switch's centre must run switch_handler and toggle on.
    #[test]
    fn switch_tap_via_dispatch_input_toggles_on() {
        let _g = mock::lock();
        mock::set_ms(0);
        let (mut world, root, _slider, switch) = build_widget_world();
        world.insert_resource(MonoClock::new(mock::clock_fn));

        let bar = world
            .query::<TabBar>()
            .collect()
            .first()
            .copied()
            .expect("TabBar");
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 2;
        }
        tab_pages_system(&mut world);

        let cx = Fixed::from_int(64);
        let cy = Fixed::from_int(71);

        let now_ms = world.resource::<MonoClock>().unwrap().now_ms();
        dispatch_input(
            &mut world,
            root,
            &InputEvent::PointerDown {
                id: 0,
                x: cx,
                y: cy,
            },
            now_ms,
            128,
            128,
        );

        mock::advance_ms(50);
        let now_ms = world.resource::<MonoClock>().unwrap().now_ms();
        dispatch_input(
            &mut world,
            root,
            &InputEvent::PointerUp {
                id: 0,
                x: cx,
                y: cy,
            },
            now_ms,
            128,
            128,
        );

        let pending: Vec<crate::event::gesture::GestureEvent> = world
            .resource_mut::<GestureSystem>()
            .map(|gs| gs.events.drain().collect())
            .unwrap_or_default();
        for g in &pending {
            crate::event::bubble_dispatch(&mut world, g);
        }

        let on = world.get::<Switch>(switch).map(|s| s.on).unwrap_or(false);
        assert!(
            on,
            "Switch.on should flip true after Tap at its centre. \
             pending events: {pending:?}",
        );
    }

    /// Bug repro for ESP cycle-2 toggle loss: after one full sim
    /// cycle the timeline must still fire toggles in cycle 2. Drives
    /// ~25 s of virtual time. Catches drift / state-leak regressions
    /// that survived a single-frame check but break across cycles.
    ///
    /// Note: this passes today on host but the ESP-side bug
    /// observed in v0.12.2 (cycle-2 hit_test routing toggle Tap to
    /// the TabContent wrapper instead of the Switch entity) does
    /// **not** reproduce here yet. Pending: tighten the fixture
    /// further (full App::run + FramebufSurface + sync_delta_time_ms)
    /// so it matches the device runtime path 1:1.
    #[test]
    fn cycle_2_switch_toggle_still_works() {
        let _g = mock::lock();
        mock::set_ms(0);
        let (mut world, _root, _slider, switch) = build_widget_world();
        world.insert_resource(MonoClock::new(mock::clock_fn));

        world.insert_resource(
            SimTimeline::new(alloc::vec![
                SimAction::Wait(500),
                SimAction::Tap(crate::types::Point::new(64, 7)),
                SimAction::Wait(1500),
                SimAction::Drag {
                    from: crate::types::Point::new(14, 71),
                    to: crate::types::Point::new(116, 71),
                    duration_ms: 600,
                    ease: crate::anim::ease::ease_in_out_cubic,
                },
                SimAction::Wait(800),
                SimAction::Tap(crate::types::Point::new(108, 7)),
                SimAction::Wait(1200),
                SimAction::Tap(crate::types::Point::new(64, 71)),
                SimAction::Wait(800),
                SimAction::Tap(crate::types::Point::new(64, 71)),
                SimAction::Wait(800),
                SimAction::Tap(crate::types::Point::new(20, 7)),
                SimAction::Wait(800),
                SimAction::Drag {
                    from: crate::types::Point::new(64, 100),
                    to: crate::types::Point::new(64, 30),
                    duration_ms: 700,
                    ease: crate::anim::ease::ease_in_out_cubic,
                },
                SimAction::Wait(800),
                SimAction::Drag {
                    from: crate::types::Point::new(64, 30),
                    to: crate::types::Point::new(64, 100),
                    duration_ms: 700,
                    ease: crate::anim::ease::ease_in_out_cubic,
                },
                SimAction::Wait(800),
            ])
            .looping(true),
        );

        let cycle_ms: u64 = 9700;
        let step: u64 = 16;
        let total_frames: u64 = (cycle_ms * 25 / 10) / step; // ~2.5 cycles

        let viewport = crate::types::Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, _root, &viewport);

        let mut cycle1_toggles = 0u32;
        let mut cycle2_toggles = 0u32;
        let mut last_on = false;

        for frame in 0..total_frames {
            sim_timeline_system(&mut world);
            crate::event::scroll::scroll_inertia_system(&mut world);
            tab_pages_system(&mut world);
            crate::components::switch::switch_init_system(&mut world);
            crate::components::switch::animate_switch_bg_t_system(&mut world);
            crate::components::switch::animate_thumb_x_system(&mut world);
            crate::widget::render_system::update_layout(&mut world, _root, &viewport);

            let on = world.get::<Switch>(switch).map(|s| s.on).unwrap_or(false);
            if on != last_on {
                let elapsed = frame * step;
                if elapsed < cycle_ms {
                    cycle1_toggles += 1;
                } else if elapsed < 2 * cycle_ms {
                    cycle2_toggles += 1;
                }
                last_on = on;
            }
            mock::advance_ms(step);
        }

        assert!(
            cycle1_toggles >= 2,
            "cycle 1 expected ≥ 2 toggles, got {cycle1_toggles}",
        );
        assert!(
            cycle2_toggles >= 2,
            "cycle 2 expected ≥ 2 toggles, got {cycle2_toggles} \
             (cycle1 saw {cycle1_toggles}). Same Switch entity, same \
             timeline — drift is the bug we want to catch.",
        );
    }
}
