use alloc::vec::Vec;

use crate::anim::EaseFn;
use crate::ecs::{Entity, World};
use crate::types::{DimPoint, Fixed};

use super::gesture::GestureSystem;
use super::hit_test::hit_test;
use super::input::InputEvent;
use crate::types::Point;
use crate::widget::ComputedRect;

/// `None` when the entity isn't in the live layout (Hidden, detached,
/// not yet mounted). Caller treats this as "retry next frame".
fn resolve_dim_point(world: &World, p: DimPoint, anchor: Option<Entity>) -> Option<Point> {
    let (origin, parent_w, parent_h) = match anchor {
        Some(e) => {
            let rect = world.get::<ComputedRect>(e)?.0;
            (
                Point {
                    x: rect.x,
                    y: rect.y,
                },
                rect.w,
                rect.h,
            )
        }
        None => (
            Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            Fixed::ZERO,
            Fixed::ZERO,
        ),
    };
    let (lx, ly) = p.resolve(parent_w, parent_h);
    Some(Point {
        x: origin.x + lx,
        y: origin.y + ly,
    })
}

fn anchored_center(world: &World, anchor: Option<Entity>, center: Point) -> Point {
    let Some(e) = anchor else { return center };
    let Some(rect) = world.get::<ComputedRect>(e).map(|r| r.0) else {
        return center;
    };
    let inside = center.x >= rect.x
        && center.x < rect.x + rect.w
        && center.y >= rect.y
        && center.y < rect.y + rect.h;
    if inside {
        center
    } else {
        Point {
            x: rect.x + rect.w / Fixed::from_int(2),
            y: rect.y + rect.h / Fixed::from_int(2),
        }
    }
}

fn anchored_pinch_dist(world: &World, anchor: Option<Entity>, center: Point, dist: Fixed) -> Fixed {
    let Some(e) = anchor else { return dist };
    let Some(rect) = world.get::<ComputedRect>(e).map(|r| r.0) else {
        return dist;
    };
    let left = center.x - rect.x;
    let right = rect.x + rect.w - center.x;
    let max_half = left.min(right).max(Fixed::ZERO);
    let max_dist = max_half * Fixed::from_int(2);
    if max_dist <= Fixed::ZERO {
        Fixed::ZERO
    } else {
        dist.clamp(Fixed::ZERO, max_dist)
    }
}

fn anchored_rotate_radius(
    world: &World,
    anchor: Option<Entity>,
    center: Point,
    radius: Fixed,
) -> Fixed {
    let Some(e) = anchor else { return radius };
    let Some(rect) = world.get::<ComputedRect>(e).map(|r| r.0) else {
        return radius;
    };
    let left = center.x - rect.x;
    let right = rect.x + rect.w - center.x;
    let top = center.y - rect.y;
    let bottom = rect.y + rect.h - center.y;
    let max_radius = left.min(right).min(top).min(bottom).max(Fixed::ZERO);
    radius.clamp(Fixed::ZERO, max_radius)
}

#[derive(Clone, Copy)]
enum ResolvedAction {
    Tap(Point),
    Drag {
        from: Point,
        to: Point,
        duration_ms: u16,
        ease: EaseFn,
    },
    MoveTo {
        from: Point,
        to: Point,
        duration_ms: u16,
        ease: EaseFn,
    },
    Rotate {
        ticks: i16,
        step_ms: u16,
        ease_fn: Option<fn(Fixed) -> Fixed>,
    },
    Pinch {
        center: Point,
        from_dist: Fixed,
        to_dist: Fixed,
        duration_ms: u16,
        ease: EaseFn,
    },
    RotateGesture {
        center: Point,
        radius: Fixed,
        from_angle: Fixed,
        to_angle: Fixed,
        duration_ms: u16,
        ease: EaseFn,
    },
    RotaryClick,
    Wait(u32),
}

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

#[crate::system(order = SIM_INPUT)]
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

/// `point` is screen coords when `anchor` is `None`, entity-local when
/// anchored. `Dimension::Percent` resolves against the entity's rect.
#[derive(Clone, Copy)]
pub struct TapAction {
    pub point: DimPoint,
    pub anchor: Option<Entity>,
}

#[derive(Clone, Copy)]
pub struct DragAction {
    pub from: DimPoint,
    pub to: DimPoint,
    pub duration_ms: u16,
    pub ease: EaseFn,
    pub anchor: Option<Entity>,
}

/// Animated cursor move without pressing — emits `PointerMove` along
/// the path so `hover_system` flips `Hovered` markers between widgets.
#[derive(Clone, Copy)]
pub struct MoveAction {
    pub from: DimPoint,
    pub to: DimPoint,
    pub duration_ms: u16,
    pub ease: EaseFn,
    pub anchor: Option<Entity>,
}

/// Encoder / Digital Crown rotation. `ticks` is the signed total
/// number of detents to dispatch.
///
/// Two timing modes:
/// - `ease_fn = None`: detents at fixed `step_ms` intervals (legacy
///   behaviour, kept for `SimAction::rotate`).
/// - `ease_fn = Some(f)`: detents distributed across `step_ms × ticks`
///   total time according to `f(t/total) → progress in [0, 1]`. A
///   linear `ease_fn` is equivalent to fixed-step; non-linear curves
///   simulate a real encoder's accel / decel envelope.
#[derive(Clone, Copy)]
pub struct RotateAction {
    pub ticks: i16,
    pub step_ms: u16,
    pub ease_fn: Option<fn(Fixed) -> Fixed>,
}

#[derive(Clone, Copy)]
pub struct PinchAction {
    pub center: DimPoint,
    pub from_dist: Fixed,
    pub to_dist: Fixed,
    pub duration_ms: u16,
    pub ease: EaseFn,
    pub anchor: Option<Entity>,
}

#[derive(Clone, Copy)]
pub struct RotateGestureAction {
    pub center: DimPoint,
    pub radius: Fixed,
    pub from_angle: Fixed,
    pub to_angle: Fixed,
    pub duration_ms: u16,
    pub ease: EaseFn,
    pub anchor: Option<Entity>,
}

#[derive(Clone, Copy)]
pub enum SimAction {
    Tap(TapAction),
    Drag(DragAction),
    MoveTo(MoveAction),
    Rotate(RotateAction),
    Pinch(PinchAction),
    RotateGesture(RotateGestureAction),
    RotaryClick,
    Wait(u32),
}

impl SimAction {
    pub fn tap(point: impl Into<DimPoint>) -> Self {
        Self::Tap(TapAction {
            point: point.into(),
            anchor: None,
        })
    }

    pub fn drag(
        from: impl Into<DimPoint>,
        to: impl Into<DimPoint>,
        duration_ms: u16,
        ease: EaseFn,
    ) -> Self {
        Self::Drag(DragAction {
            from: from.into(),
            to: to.into(),
            duration_ms,
            ease,
            anchor: None,
        })
    }

    pub fn move_to(
        from: impl Into<DimPoint>,
        to: impl Into<DimPoint>,
        duration_ms: u16,
        ease: EaseFn,
    ) -> Self {
        Self::MoveTo(MoveAction {
            from: from.into(),
            to: to.into(),
            duration_ms,
            ease,
            anchor: None,
        })
    }

    pub fn rotate(ticks: i16, step_ms: u16) -> Self {
        Self::Rotate(RotateAction {
            ticks,
            step_ms,
            ease_fn: None,
        })
    }

    /// Detents distributed over `total_ms` (rounded up to the next
    /// multiple of `|ticks|`) by `ease_fn`. `ease::linear` matches
    /// the fixed-tempo `rotate(ticks, total_ms / |ticks|)`.
    pub fn rotate_smooth(ticks: i16, total_ms: u16, ease_fn: fn(Fixed) -> Fixed) -> Self {
        let abs = ticks.unsigned_abs().max(1);
        let step_ms = total_ms.div_ceil(abs).max(1);
        Self::Rotate(RotateAction {
            ticks,
            step_ms,
            ease_fn: Some(ease_fn),
        })
    }

    pub fn pinch(
        center: impl Into<DimPoint>,
        from_dist: Fixed,
        to_dist: Fixed,
        duration_ms: u16,
        ease: EaseFn,
    ) -> Self {
        Self::Pinch(PinchAction {
            center: center.into(),
            from_dist,
            to_dist,
            duration_ms,
            ease,
            anchor: None,
        })
    }

    pub fn rotate_gesture(
        center: impl Into<DimPoint>,
        radius: Fixed,
        from_angle: Fixed,
        to_angle: Fixed,
        duration_ms: u16,
        ease: EaseFn,
    ) -> Self {
        Self::RotateGesture(RotateGestureAction {
            center: center.into(),
            radius,
            from_angle,
            to_angle,
            duration_ms,
            ease,
            anchor: None,
        })
    }

    pub fn rotary_click() -> Self {
        Self::RotaryClick
    }

    pub fn wait(ms: u32) -> Self {
        Self::Wait(ms)
    }

    /// Anchor the action's coordinates to an entity's `ComputedRect`.
    /// No-op on actions without coordinates.
    pub fn on(mut self, e: Entity) -> Self {
        match &mut self {
            Self::Tap(t) => t.anchor = Some(e),
            Self::Drag(d) => d.anchor = Some(e),
            Self::MoveTo(m) => m.anchor = Some(e),
            Self::Pinch(p) => p.anchor = Some(e),
            Self::RotateGesture(r) => r.anchor = Some(e),
            Self::Rotate(_) | Self::RotaryClick | Self::Wait(_) => {}
        }
        self
    }
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
    drag_move_emitted: bool,
    rotate_emitted: u16,
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
                SimAction::Drag(d) => d.duration_ms as u32,
                SimAction::MoveTo(m) => m.duration_ms as u32,
                SimAction::Rotate(r) => (r.ticks.unsigned_abs() as u32) * (r.step_ms as u32),
                SimAction::Pinch(p) => p.duration_ms as u32,
                SimAction::RotateGesture(r) => r.duration_ms as u32,
                SimAction::RotaryClick => 100,
                SimAction::Wait(ms) => *ms,
            };
        }
        Self {
            entries,
            cursor: 0,
            action_elapsed_ms: 0,
            action_started: false,
            drag_move_emitted: false,
            rotate_emitted: 0,
            start_ms: None,
            looping: false,
            total_ms: t,
        }
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// `App::run` swallows real backend input while this is true.
    pub fn is_running(&self) -> bool {
        self.looping || self.cursor < self.entries.len()
    }
}

#[crate::system(order = SIM_INPUT)]
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

    // Tight scope releases the &mut Timeline borrow before entity_centre.
    let (entry, action_elapsed, action_started) = {
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
                tl.drag_move_emitted = false;
                tl.rotate_emitted = 0;
                tl.start_ms = Some(now_ms);
            }
            return;
        }

        let entry = tl.entries[tl.cursor];
        if elapsed < entry.start_ms {
            return;
        }
        (entry, elapsed - entry.start_ms, tl.action_started)
    };

    // Each (point, anchor) pair resolves to a concrete screen Point.
    // None means "wait for layout, retry next frame".
    let resolved: Option<ResolvedAction> = match entry.action {
        SimAction::Tap(t) => resolve_dim_point(world, t.point, t.anchor).map(ResolvedAction::Tap),
        SimAction::Drag(d) => {
            let from = resolve_dim_point(world, d.from, d.anchor);
            let to = resolve_dim_point(world, d.to, d.anchor);
            match (from, to) {
                (Some(f), Some(t)) => Some(ResolvedAction::Drag {
                    from: f,
                    to: t,
                    duration_ms: d.duration_ms,
                    ease: d.ease,
                }),
                _ => None,
            }
        }
        SimAction::MoveTo(m) => {
            let from = resolve_dim_point(world, m.from, m.anchor);
            let to = resolve_dim_point(world, m.to, m.anchor);
            match (from, to) {
                (Some(f), Some(t)) => Some(ResolvedAction::MoveTo {
                    from: f,
                    to: t,
                    duration_ms: m.duration_ms,
                    ease: m.ease,
                }),
                _ => None,
            }
        }
        SimAction::Rotate(r) => Some(ResolvedAction::Rotate {
            ticks: r.ticks,
            step_ms: r.step_ms,
            ease_fn: r.ease_fn,
        }),
        SimAction::Pinch(p) => resolve_dim_point(world, p.center, p.anchor).map(|c| {
            let c = anchored_center(world, p.anchor, c);
            ResolvedAction::Pinch {
                center: c,
                from_dist: anchored_pinch_dist(world, p.anchor, c, p.from_dist),
                to_dist: anchored_pinch_dist(world, p.anchor, c, p.to_dist),
                duration_ms: p.duration_ms,
                ease: p.ease,
            }
        }),
        SimAction::RotateGesture(r) => resolve_dim_point(world, r.center, r.anchor).map(|c| {
            let c = anchored_center(world, r.anchor, c);
            ResolvedAction::RotateGesture {
                center: c,
                radius: anchored_rotate_radius(world, r.anchor, c, r.radius),
                from_angle: r.from_angle,
                to_angle: r.to_angle,
                duration_ms: r.duration_ms,
                ease: r.ease,
            }
        }),
        SimAction::RotaryClick => Some(ResolvedAction::RotaryClick),
        SimAction::Wait(ms) => Some(ResolvedAction::Wait(ms)),
    };

    // ComputedRect missing — wait for layout instead of firing at (0, 0).
    // Anchored entities frequently come from a still-Hidden tab subtree,
    // so we keep waiting indefinitely; the timeline's own Wait actions
    // remain the only way to budget time.
    let Some(action) = resolved else {
        return;
    };

    match action {
        ResolvedAction::Tap(pt) => {
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
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
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            }
        }
        ResolvedAction::Drag {
            from,
            to,
            duration_ms,
            ease,
        } => {
            let move_emitted = world
                .resource::<SimTimeline>()
                .map(|tl| tl.drag_move_emitted)
                .unwrap_or(false);
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                    tl.drag_move_emitted = false;
                }
                let event = InputEvent::PointerDown {
                    id: 0,
                    x: from.x,
                    y: from.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= duration_ms as u32 && move_emitted {
                let event = InputEvent::PointerUp {
                    id: 0,
                    x: to.x,
                    y: to.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            } else {
                // Catch-up branch (elapsed ≥ duration, no move yet) clamps t=1
                // so a sub-tick drag still emits one move before PointerUp.
                let t_raw = if action_elapsed >= duration_ms as u32 {
                    Fixed::ONE
                } else {
                    Fixed::from_raw(
                        (action_elapsed as i32) * Fixed::ONE.raw() / (duration_ms as i32),
                    )
                };
                let eased = ease(t_raw);
                let x = from.x + eased * (to.x - from.x);
                let y = from.y + eased * (to.y - from.y);
                let event = InputEvent::PointerMove { id: 0, x, y };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.drag_move_emitted = true;
                }
            }
        }
        ResolvedAction::MoveTo {
            from,
            to,
            duration_ms,
            ease,
        } => {
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
                let event = InputEvent::PointerMove {
                    id: 0,
                    x: from.x,
                    y: from.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= duration_ms as u32 {
                let event = InputEvent::PointerMove {
                    id: 0,
                    x: to.x,
                    y: to.y,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
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
        ResolvedAction::Pinch {
            center,
            from_dist,
            to_dist,
            duration_ms,
            ease,
        } => {
            let half = |d: Fixed| d / Fixed::from_int(2);
            let two_fingers = |d: Fixed| {
                (
                    Point {
                        x: center.x - half(d),
                        y: center.y,
                    },
                    Point {
                        x: center.x + half(d),
                        y: center.y,
                    },
                )
            };
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
                let (a, b) = two_fingers(from_dist);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            } else if action_elapsed >= duration_ms as u32 {
                let (a, b) = two_fingers(to_dist);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            } else {
                let t = Fixed::from_raw(
                    (action_elapsed as i32) * Fixed::ONE.raw() / (duration_ms as i32),
                );
                let eased = ease(t);
                let dist = from_dist + eased * (to_dist - from_dist);
                let (a, b) = two_fingers(dist);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            }
        }
        ResolvedAction::RotateGesture {
            center,
            radius,
            from_angle,
            to_angle,
            duration_ms,
            ease,
        } => {
            let two_fingers = |angle: Fixed| {
                let dx = radius * Fixed::cos_rad(angle);
                let dy = radius * Fixed::sin_rad(angle);
                (
                    Point {
                        x: center.x - dx,
                        y: center.y - dy,
                    },
                    Point {
                        x: center.x + dx,
                        y: center.y + dy,
                    },
                )
            };
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
                let (a, b) = two_fingers(from_angle);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            } else if action_elapsed >= duration_ms as u32 {
                let (a, b) = two_fingers(to_angle);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            } else {
                let t = Fixed::from_raw(
                    (action_elapsed as i32) * Fixed::ONE.raw() / (duration_ms as i32),
                );
                let eased = ease(t);
                let angle = from_angle + eased * (to_angle - from_angle);
                let (a, b) = two_fingers(angle);
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::dispatch_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            }
        }
        ResolvedAction::Rotate {
            ticks,
            step_ms,
            ease_fn,
        } => {
            let total = ticks.unsigned_abs();
            if total == 0 {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            } else {
                let step = step_ms.max(1) as u32;
                let total_ms = step * total as u32;
                // ease_fn maps elapsed/total ∈ [0, 1] to a progress
                // value also in [0, 1]; multiply by total detents for
                // the current target. Linear ease and `ease_fn = None`
                // both reduce to the legacy `elapsed / step` formula.
                let target = if let Some(ease) = ease_fn {
                    let t = if total_ms == 0 {
                        Fixed::ONE
                    } else {
                        Fixed::from_int(action_elapsed.min(total_ms) as i32)
                            / Fixed::from_int(total_ms as i32)
                    };
                    let progress = ease(t).clamp(Fixed::ZERO, Fixed::ONE);
                    let det_f = progress * Fixed::from_int(total as i32);
                    (det_f.to_int() as u16).min(total)
                } else {
                    ((action_elapsed / step) as u16).min(total)
                };
                let already = world
                    .resource::<SimTimeline>()
                    .map(|t| t.rotate_emitted)
                    .unwrap_or(0);
                let sign: i16 = if ticks >= 0 { 1 } else { -1 };
                for _ in already..target {
                    let event = InputEvent::Rotary { id: 0, delta: sign };
                    super::dispatch_input(world, root, &event, now_ms, lw, lh);
                }
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.rotate_emitted = target;
                    if target >= total {
                        tl.cursor += 1;
                        tl.action_started = false;
                        tl.rotate_emitted = 0;
                    }
                }
            }
        }
        ResolvedAction::RotaryClick => {
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
                let event = InputEvent::Key {
                    code: crate::event::input::KEY_ROTARY_PRESS,
                    pressed: true,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= 50 {
                let event = InputEvent::Key {
                    code: crate::event::input::KEY_ROTARY_PRESS,
                    pressed: false,
                };
                super::dispatch_input(world, root, &event, now_ms, lw, lh);
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
            }
        }
        ResolvedAction::Wait(ms) => {
            if action_elapsed >= ms {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
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
    use crate::widget::ComputedRect;

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
    fn move_to_emits_pointer_move_without_pressing() {
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::move_to(
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(10),
            },
            Point {
                x: Fixed::from_int(50),
                y: Fixed::from_int(10),
            },
            200,
            crate::anim::ease::linear,
        )]));

        sim_timeline_system(&mut world);
        let cursor = world
            .resource::<crate::event::PointerCursor>()
            .copied()
            .expect("cursor seeded by sim");
        assert_eq!(cursor.x, Fixed::from_int(10));
        assert!(!cursor.down, "MoveTo must keep cursor.down false");

        for _ in 0..7 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        let mid = world
            .resource::<crate::event::PointerCursor>()
            .copied()
            .unwrap();
        assert!(
            mid.x > Fixed::from_int(10) && mid.x < Fixed::from_int(50),
            "cursor must interpolate between endpoints, got {:?}",
            mid.x,
        );
        assert!(!mid.down);

        for _ in 0..20 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        let end = world
            .resource::<crate::event::PointerCursor>()
            .copied()
            .unwrap();
        assert_eq!(end.x, Fixed::from_int(50));
        assert!(!end.down);
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "MoveTo timeline cursor must advance past completion",
        );
    }

    #[test]
    fn rotate_advances_after_step_ms_per_tick() {
        let _g = mock::lock();
        let mut world = setup_world();
        // 5 ticks × 20 ms each = 100 ms total.
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::rotate(5, 20)]));

        sim_timeline_system(&mut world);
        assert_eq!(world.resource::<SimTimeline>().unwrap().cursor, 0);

        for _ in 0..3 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().rotate_emitted,
            2,
            "rotate_emitted must equal elapsed_ms / step_ms",
        );
        assert_eq!(world.resource::<SimTimeline>().unwrap().cursor, 0);

        for _ in 0..6 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "rotate failed to advance after total duration",
        );
        assert_eq!(world.resource::<SimTimeline>().unwrap().rotate_emitted, 0);
    }

    #[test]
    fn rotate_smooth_with_linear_ease_matches_rotate() {
        // Linear ease is the identity timing curve, so rotate_smooth
        // with linear must emit detents at the same relative tempo as
        // the legacy rotate(...).
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::rotate_smooth(
            4,
            80,
            crate::anim::ease::linear,
        )]));

        sim_timeline_system(&mut world);
        for _ in 0..2 {
            mock::advance_ms(20);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().rotate_emitted,
            2,
            "linear ease at 50% time → 2 of 4 detents",
        );
    }

    #[test]
    fn rotate_smooth_back_loaded_curve_stalls_then_finishes() {
        // ease_in_out_cubic is back-loaded: at t=0.25 progress is far
        // below 0.25, so detent emissions should lag the linear case.
        // Pin the contract by checking that no detents have fired
        // halfway through total_ms when ease_in_out_cubic is used,
        // while linear would have fired half by then.
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::rotate_smooth(
            4,
            100,
            crate::anim::ease::ease_in_out_cubic,
        )]));

        sim_timeline_system(&mut world);
        // 25% elapsed: ease_in_out_cubic(0.25) ≈ 0.0625 → 0 detents.
        for _ in 0..1 {
            mock::advance_ms(25);
            sim_timeline_system(&mut world);
        }
        let mid_emitted = world.resource::<SimTimeline>().unwrap().rotate_emitted;
        assert!(
            mid_emitted < 2,
            "ease_in_out_cubic at 25% should not have emitted half the detents (got {mid_emitted})",
        );

        // Run past total_ms; all 4 detents must have fired and the
        // cursor must advance.
        for _ in 0..8 {
            mock::advance_ms(20);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "rotate_smooth must fully complete after total_ms",
        );
    }

    #[test]
    fn rotate_smooth_non_divisible_total_rounds_up_step_ms() {
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::rotate_smooth(
            3,
            100,
            crate::anim::ease::linear,
        )]));

        sim_timeline_system(&mut world);
        for _ in 0..9 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "rotate_smooth(3, 100) must retire after ~102 ms",
        );
    }

    #[test]
    fn rotary_click_emits_key_pair() {
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::rotary_click()]));

        sim_timeline_system(&mut world);
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            0,
            "RotaryClick must not advance on first frame (pressed phase)",
        );

        // Hold past the 50 ms release threshold.
        for _ in 0..5 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            1,
            "RotaryClick must advance after release fires",
        );
    }

    #[test]
    fn move_to_does_not_bump_event_seq() {
        let _g = mock::lock();
        let mut world = setup_world();
        world.insert_resource(SimTimeline::new(alloc::vec![SimAction::move_to(
            Point {
                x: Fixed::from_int(0),
                y: Fixed::ZERO,
            },
            Point {
                x: Fixed::from_int(40),
                y: Fixed::ZERO,
            },
            100,
            crate::anim::ease::linear,
        )]));

        sim_timeline_system(&mut world);
        let initial_seq = world
            .resource::<crate::event::PointerCursor>()
            .map(|c| c.event_seq)
            .unwrap_or(0);

        for _ in 0..10 {
            mock::advance_ms(16);
            sim_timeline_system(&mut world);
        }
        let final_seq = world
            .resource::<crate::event::PointerCursor>()
            .map(|c| c.event_seq)
            .unwrap_or(0);
        assert_eq!(
            initial_seq, final_seq,
            "PointerMove must not bump event_seq; only Down/Up do",
        );
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
                SimAction::wait(100),
                SimAction::tap((10, 10)),
                SimAction::wait(800),
                SimAction::tap((20, 20)),
                SimAction::wait(100),
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

    fn spawn_widget(
        world: &mut World,
        parent: Option<Entity>,
        style: crate::widget::Style,
    ) -> Entity {
        use crate::widget::{Children, Parent, Widget};
        let e = world.spawn();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(children) = world.get_mut::<Children>(p) {
                children.0.push(e);
            } else {
                world.insert(p, Children(alloc::vec![e]));
            }
        }
        e
    }

    fn install_root(world: &mut World) -> Entity {
        use crate::layout::LayoutStyle;
        use crate::types::Dimension;
        let root = spawn_widget(
            world,
            None,
            crate::widget::Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(128)),
                    height: Dimension::Px(Fixed::from_int(128)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert_resource(SimRootFallback(root));
        root
    }

    #[test]
    fn tap_on_resolves_to_entity_centre() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(40)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(60)),
                    height: Dimension::Px(Fixed::from_int(30)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::tap(DimPoint::CENTER).on(target),
        ]));
        for _ in 0..10 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let tl = world.resource::<SimTimeline>().unwrap();
        assert!(tl.cursor >= 1, "TapOn never advanced cursor");
    }

    #[test]
    fn tap_on_waits_indefinitely_for_computed_rect() {
        let _g = mock::lock();
        let mut world = setup_world();
        let target = world.spawn();

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::tap(DimPoint::CENTER).on(target),
        ]));
        for _ in 0..50 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }
        assert_eq!(
            world.resource::<SimTimeline>().unwrap().cursor,
            0,
            "TapOn must wait indefinitely so a deferred-layout target still fires once visible",
        );
    }

    #[test]
    fn drag_on_anchors_endpoints_to_entity() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(10)),
                    top: Dimension::Px(Fixed::from_int(10)),
                    width: Dimension::Px(Fixed::from_int(20)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::drag(
                DimPoint::CENTER,
                DimPoint::px(60, 10),
                100,
                crate::anim::ease::linear,
            )
            .on(target),
        ]));

        for _ in 0..15 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }
        assert!(
            world.resource::<SimTimeline>().unwrap().cursor >= 1,
            "DragOn never completed",
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
    use crate::surface::InputEvent;
    use crate::types::Dimension;
    use crate::widget::builder::WidgetBuilder;
    use crate::widget::{Children, Parent};

    fn build_widget_world() -> (World, Entity, Entity, Entity) {
        let mut app = crate::app::App::headless(128, 128);
        app.with_default_widgets();
        let mut world = app.world;

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

    /// Pins the per-entity-Vec walk-alignment invariant. Tree:
    ///
    ///   root  (column, 60x90)
    ///   ├── hidden  (Hidden, ScrollOffset.y=30)
    ///   │   └── decoy
    ///   ├── visible_a
    ///   └── target
    ///
    /// entities (build_rects skips Hidden) = [root, visible_a, target].
    /// Buggy compute_scroll walked into the Hidden subtree; when it
    /// reached `decoy` the accumulator was (0, 30) and it wrote
    /// that into offsets[2] — `target`'s slot. The fix gates the
    /// scroll/transform walks on the same Widget+!Hidden+Style
    /// triple build_rects uses, so offsets stay (0,0) and Tap at
    /// target's centre hits target.
    #[test]
    fn hit_test_skips_hidden_subtree_scroll_offset() {
        use crate::event::scroll::ScrollOffset;
        use crate::widget::{Children, Hidden, Parent};
        let mut world = World::new();
        let mk = |w: &mut World, h: i32| {
            WidgetBuilder::new(w)
                .layout(LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(60),
                    height: Dimension::px(h),
                    ..Default::default()
                })
                .id()
        };
        let attach = |w: &mut World, parent: Entity, child: Entity| {
            w.insert(child, Parent(parent));
            if let Some(c) = w.get_mut::<Children>(parent) {
                c.0.push(child);
            }
        };

        let root = mk(&mut world, 90);
        let hidden = mk(&mut world, 30);
        let decoy = mk(&mut world, 30);
        let visible_a = mk(&mut world, 30);
        let target = mk(&mut world, 30);
        attach(&mut world, hidden, decoy);
        attach(&mut world, root, hidden);
        attach(&mut world, root, visible_a);
        attach(&mut world, root, target);
        world.insert(hidden, Hidden);
        world.insert(
            hidden,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(30),
            },
        );

        let hit = hit_test(
            &world,
            root,
            Fixed::from_int(30),
            Fixed::from_int(45),
            60,
            90,
        );
        assert_eq!(
            hit,
            Some(target),
            "hit at target centre returned {hit:?}; index alignment \
             between build_rects and compute_scroll_offsets is broken",
        );
    }

    /// Regression: scrolled-past row must not steal taps from a tabbar above.
    #[test]
    fn hit_test_clips_scrolled_child_to_container_rect() {
        use crate::event::scroll::ScrollOffset;
        use crate::layout::Position;
        use crate::widget::{Children, Parent};
        let mut world = World::new();
        let mk = |w: &mut World, width: i32, h: i32| {
            WidgetBuilder::new(w)
                .layout(LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(width),
                    height: Dimension::px(h),
                    ..Default::default()
                })
                .id()
        };
        let attach = |w: &mut World, parent: Entity, child: Entity| {
            w.insert(child, Parent(parent));
            if let Some(c) = w.get_mut::<Children>(parent) {
                c.0.push(child);
            }
        };

        let root = mk(&mut world, 100, 100);
        let tab = mk(&mut world, 100, 20);
        let scroll = mk(&mut world, 100, 80);
        attach(&mut world, root, tab);
        attach(&mut world, root, scroll);

        let row = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                position: Position::Absolute,
                top: Dimension::px(80),
                left: Dimension::px(0),
                width: Dimension::px(100),
                height: Dimension::px(40),
                ..Default::default()
            })
            .id();
        attach(&mut world, scroll, row);

        // tabbar y=[0,20). row layout y=100, ScrollOffset 95 → visual y=5.
        world.insert(
            scroll,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(95),
            },
        );

        let hit = hit_test(
            &world,
            root,
            Fixed::from_int(50),
            Fixed::from_int(10),
            100,
            100,
        );
        assert_eq!(hit, Some(tab));
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

    /// 100 sequential Tap events at the Switch's centre must flip
    /// Switch.on exactly 100 times. Catches "stuck after N taps"
    /// regressions: handler not idempotent on Down/Up pairing,
    /// recognizer carrying stale state across taps, animation
    /// systems blocking subsequent toggles.
    #[test]
    fn switch_n_tap_toggles_n_times() {
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
        let mut flips = 0u32;
        let mut prev_on = world.get::<Switch>(switch).map(|s| s.on).unwrap_or(false);

        for _ in 0..100 {
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
            // Drain & bubble pending Tap events the same way
            // sim_timeline_system does at the end of each sim tick.
            let pending: Vec<crate::event::gesture::GestureEvent> = world
                .resource_mut::<GestureSystem>()
                .map(|gs| gs.events.drain().collect())
                .unwrap_or_default();
            for g in &pending {
                crate::event::bubble_dispatch(&mut world, g);
            }
            // Gap between taps so the recognizer fully resets.
            mock::advance_ms(150);

            let on = world.get::<Switch>(switch).map(|s| s.on).unwrap_or(false);
            if on != prev_on {
                flips += 1;
            }
            prev_on = on;
        }

        assert_eq!(
            flips, 100,
            "100 Taps should produce 100 toggles, got {flips}",
        );
    }

    /// slider_handler must clamp ratio to [0, 1] for any Tap x —
    /// inside the rect, exactly on either edge, and one pixel past
    /// either edge. Floor at left, ceiling at right, no overflow.
    #[test]
    fn slider_handler_clamps_ratio_at_boundaries() {
        use crate::components::slider::{Slider, slider_handler};
        use crate::event::gesture::GestureEvent;
        use crate::types::Rect;
        use crate::widget::ComputedRect;

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
        world.insert(
            e,
            ComputedRect(Rect {
                x: Fixed::from_int(20),
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );

        let cases: &[(Fixed, i32)] = &[
            (Fixed::from_int(19), 0),     // 1 px left of rect
            (Fixed::from_int(20), 0),     // exact left edge
            (Fixed::from_int(70), 500),   // centre
            (Fixed::from_int(120), 1000), // exact right edge
            (Fixed::from_int(121), 1000), // 1 px past right
            (Fixed::from_int(-50), 0),
            (Fixed::from_int(10_000), 1000),
        ];
        for &(x, want_promille) in cases {
            let ev = GestureEvent::Tap {
                x,
                y: Fixed::from_int(10),
                target: e,
            };
            assert!(slider_handler(&mut world, e, &ev), "handler accepts Tap");
            let r = world.get::<Slider>(e).unwrap().ratio();
            assert!(
                r >= Fixed::ZERO && r <= Fixed::ONE,
                "ratio out of [0,1] at x={}: {:?}",
                x.to_int(),
                r,
            );
            let got = (r * Fixed::from_int(1000)).to_int();
            assert!(
                (got - want_promille).abs() <= 5,
                "x={}: got promille={}, want={}",
                x.to_int(),
                got,
                want_promille,
            );
        }
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
                SimAction::wait(500),
                SimAction::tap((64, 7)),
                SimAction::wait(1500),
                SimAction::drag(
                    (14, 71),
                    (116, 71),
                    600,
                    crate::anim::ease::ease_in_out_cubic
                ),
                SimAction::wait(800),
                SimAction::tap((108, 7)),
                SimAction::wait(1200),
                SimAction::tap((64, 71)),
                SimAction::wait(800),
                SimAction::tap((64, 71)),
                SimAction::wait(800),
                SimAction::tap((20, 7)),
                SimAction::wait(800),
                SimAction::drag(
                    (64, 100),
                    (64, 30),
                    700,
                    crate::anim::ease::ease_in_out_cubic
                ),
                SimAction::wait(800),
                SimAction::drag(
                    (64, 30),
                    (64, 100),
                    700,
                    crate::anim::ease::ease_in_out_cubic
                ),
                SimAction::wait(800),
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

    #[derive(Default)]
    struct PinchProbe {
        accum_scale: Fixed,
        accum_scale64: crate::types::Fixed64,
        per_frame: Vec<Fixed>,
        deltas: Vec<crate::types::Fixed64>,
        rotate_deltas: Vec<Fixed>,
    }

    fn pinch_probe_handler(
        world: &mut World,
        _entity: Entity,
        event: &crate::event::gesture::GestureEvent,
    ) -> bool {
        if let crate::event::gesture::GestureEvent::Pinch { scale_delta, .. } = event {
            if let Some(p) = world.resource_mut::<PinchProbe>() {
                if p.accum_scale == Fixed::ZERO {
                    p.accum_scale = Fixed::ONE;
                    p.accum_scale64 = crate::types::Fixed64::ONE;
                }
                p.deltas.push(*scale_delta);
                p.accum_scale64 = p.accum_scale64 * *scale_delta;
                p.accum_scale = p.accum_scale64.to_fixed();
                p.per_frame.push(p.accum_scale);
            }
            return true;
        }
        if let crate::event::gesture::GestureEvent::Rotate { angle, .. } = event {
            if let Some(p) = world.resource_mut::<PinchProbe>() {
                p.rotate_deltas.push(*angle);
            }
            return true;
        }
        false
    }

    #[test]
    fn pinch_two_rounds_handler_scale_is_continuous() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        let center = Point {
            x: Fixed::from_int(64),
            y: Fixed::from_int(64),
        };
        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::pinch(
                center,
                Fixed::from_int(20),
                Fixed::from_int(70),
                200,
                crate::anim::ease::linear,
            ),
            SimAction::wait(100),
            SimAction::pinch(
                center,
                Fixed::from_int(70),
                Fixed::from_int(20),
                200,
                crate::anim::ease::linear,
            ),
        ]));

        for _ in 0..40 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        assert!(
            probe.per_frame.len() >= 5,
            "Pinch fired too few times ({}); expected at least 5 across two 200ms rounds",
            probe.per_frame.len(),
        );

        let mut max_jump = Fixed::ZERO;
        for w in probe.per_frame.windows(2) {
            let prev = w[0];
            let curr = w[1];
            let ratio = if prev > curr {
                prev / curr
            } else {
                curr / prev
            };
            let jump = ratio - Fixed::ONE;
            if jump > max_jump {
                max_jump = jump;
            }
        }
        assert!(
            max_jump < Fixed::ONE / Fixed::from_int(2),
            "frame-to-frame scale jumped > 50%: max_jump = {:?}, frames = {:?}",
            max_jump,
            probe.per_frame,
        );

        let final_scale = *probe.per_frame.last().unwrap();
        let drift = if final_scale > Fixed::ONE {
            final_scale - Fixed::ONE
        } else {
            Fixed::ONE - final_scale
        };
        assert!(
            drift < Fixed::ONE / Fixed::from_int(4),
            "round-trip Pinch did not return to ~1.0×: final = {:?}",
            final_scale,
        );
    }

    #[test]
    fn pinch_demo_timeline_emits_expand_and_shrink_deltas() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        let center = Point {
            x: Fixed::from_int(64),
            y: Fixed::from_int(64),
        };
        let small = Fixed::from_int(40);
        let large = Fixed::from_int(80);
        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::pinch(
                center,
                small,
                large,
                1500,
                crate::anim::ease::ease_in_out_cubic,
            ),
            SimAction::wait(800),
            SimAction::pinch(
                center,
                large,
                small,
                1500,
                crate::anim::ease::ease_in_out_cubic,
            ),
        ]));

        for _ in 0..260 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        let has_expand = probe.deltas.iter().any(|d| *d > crate::types::Fixed64::ONE);
        let has_shrink = probe.deltas.iter().any(|d| *d < crate::types::Fixed64::ONE);
        assert!(has_expand, "no expand deltas: {:?}", probe.deltas);
        assert!(has_shrink, "no shrink deltas: {:?}", probe.deltas);
        let final_scale = probe.accum_scale;
        let drift = if final_scale > Fixed::ONE {
            final_scale - Fixed::ONE
        } else {
            Fixed::ONE - final_scale
        };
        assert!(
            drift <= Fixed::from_raw(8),
            "demo shrink/expand roundtrip should return near 1.0x, got {:?}, deltas={:?}",
            final_scale,
            probe.deltas,
        );
    }

    #[test]
    fn anchored_pinch_clamps_span_to_target_rect() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::pinch(
                DimPoint::CENTER,
                Fixed::from_int(40),
                Fixed::from_int(160),
                400,
                crate::anim::ease::linear,
            )
            .on(target)
        ]));

        for _ in 0..40 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        assert!(
            probe.deltas.iter().any(|d| *d > crate::types::Fixed64::ONE),
            "wide anchored Pinch should still hit target after clamping, got {:?}",
            probe.deltas,
        );
    }

    #[test]
    fn anchored_pinch_recenters_outside_local_center() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::pinch(
                (Dimension::px(-100), Dimension::px(-100)),
                Fixed::from_int(40),
                Fixed::from_int(80),
                400,
                crate::anim::ease::linear,
            )
            .on(target),
        ]));

        for _ in 0..40 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        assert!(
            probe.deltas.iter().any(|d| *d > crate::types::Fixed64::ONE),
            "anchored Pinch with outside local center should be recentered, got {:?}",
            probe.deltas,
        );
    }

    #[test]
    fn anchored_rotate_gesture_clamps_radius_to_target_rect() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        world.insert_resource(SimTimeline::new(alloc::vec![
            SimAction::rotate_gesture(
                DimPoint::CENTER,
                Fixed::from_int(100),
                Fixed::ZERO,
                Fixed::PI / Fixed::from_int(2),
                400,
                crate::anim::ease::linear,
            )
            .on(target),
        ]));

        for _ in 0..40 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        assert!(
            probe.rotate_deltas.iter().any(|d| *d != Fixed::ZERO),
            "wide anchored RotateGesture should still hit target after clamping, got {:?}",
            probe.rotate_deltas,
        );
    }

    #[test]
    fn pinch_rotate_demo_loop_expands_shrinks_and_rotates() {
        use crate::layout::{LayoutStyle, Position};
        use crate::types::{Dimension, Viewport};
        let _g = mock::lock();
        let mut world = setup_world();
        let root = install_root(&mut world);
        let target = spawn_widget(
            &mut world,
            Some(root),
            crate::widget::Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(88)),
                    height: Dimension::Px(Fixed::from_int(88)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            crate::event::GestureHandler {
                on_gesture: pinch_probe_handler,
            },
        );
        world.insert_resource(PinchProbe::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::widget::render_system::update_layout(&mut world, root, &viewport);

        let center = Point {
            x: Fixed::from_int(64),
            y: Fixed::from_int(64),
        };
        let small = Fixed::from_int(40);
        let large = Fixed::from_int(80);
        let radius = Fixed::from_int(30);
        world.insert_resource(
            SimTimeline::new(alloc::vec![
                SimAction::pinch(
                    center,
                    small,
                    large,
                    1500,
                    crate::anim::ease::ease_in_out_cubic,
                ),
                SimAction::wait(800),
                SimAction::pinch(
                    center,
                    large,
                    small,
                    1500,
                    crate::anim::ease::ease_in_out_cubic,
                ),
                SimAction::wait(800),
                SimAction::rotate_gesture(
                    center,
                    radius,
                    Fixed::ZERO,
                    Fixed::PI / Fixed::from_int(2),
                    1500,
                    crate::anim::ease::ease_in_out_cubic,
                ),
                SimAction::wait(800),
                SimAction::rotate_gesture(
                    center,
                    radius,
                    Fixed::PI / Fixed::from_int(2),
                    Fixed::ZERO,
                    1500,
                    crate::anim::ease::ease_in_out_cubic,
                ),
                SimAction::wait(800),
            ])
            .looping(true),
        );

        for _ in 0..1100 {
            sim_timeline_system(&mut world);
            mock::advance_ms(16);
        }

        let probe = world.resource::<PinchProbe>().expect("probe present");
        let has_expand = probe.deltas.iter().any(|d| *d > crate::types::Fixed64::ONE);
        let has_shrink = probe.deltas.iter().any(|d| *d < crate::types::Fixed64::ONE);
        let has_rotate = probe.rotate_deltas.iter().any(|d| *d != Fixed::ZERO);
        assert!(has_expand, "loop has no expand deltas: {:?}", probe.deltas);
        assert!(has_shrink, "loop has no shrink deltas: {:?}", probe.deltas);
        assert!(
            has_rotate,
            "loop has no rotate deltas: {:?}",
            probe.rotate_deltas
        );
    }
}
