use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::anim::EaseFn;
use crate::ecs::{Entity, World};
use crate::types::{DimPoint, Fixed};

use super::input::InputEvent;
use crate::types::Point;
use crate::ui::ComputedRect;

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
    commands: Rc<Vec<SimCommand>>,
    resolved_ms: Vec<u32>,
    cursor: usize,
    start_ms: Option<u32>,
    looping: bool,
    /// Input root override. `None` uses the root installed by the App.
    pub root: Option<Entity>,
}

impl SimulatedInput {
    pub fn new(commands: Vec<SimCommand>) -> Self {
        let resolved = resolve_timings(&commands);
        Self {
            commands: Rc::new(commands),
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

pub(crate) fn sim_input_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let clock_fn = world.resource::<crate::ecs::MonoClock>().map(|fc| fc.clock);
    let now_ms = clock_fn.map(|f| (f() / 1_000_000) as u32).unwrap_or(0);

    let (commands, due, root, screen_w, screen_h) = {
        let Some(sim) = world.resource_mut::<SimulatedInput>() else {
            return Ok(());
        };
        if sim.commands.is_empty() {
            return Ok(());
        }

        let start = *sim.start_ms.get_or_insert(now_ms);
        let elapsed = now_ms.wrapping_sub(start);

        let first = sim.cursor;
        while sim.cursor < sim.commands.len() {
            let target_ms = sim.resolved_ms[sim.cursor];
            if elapsed >= target_ms {
                sim.cursor += 1;
            } else {
                break;
            }
        }

        let due = first..sim.cursor;
        let commands = Rc::clone(&sim.commands);
        if sim.cursor >= sim.commands.len() && sim.looping {
            sim.cursor = 0;
            sim.start_ms = Some(now_ms);
        }

        let root = sim.root;
        let (sw, sh) = world
            .resource::<crate::surface::DisplayInfo>()
            .map(|d| (d.width, d.height))
            .unwrap_or((128, 128));
        (commands, due, root, sw, sh)
    };

    let root = root.or_else(|| {
        world
            .resource::<SimRootFallback>()
            .map(|fallback| fallback.0)
    });

    for command in &commands[due] {
        super::deliver_input(world, root, &command.event, now_ms, screen_w, screen_h);
    }

    super::dispatch_pending_gestures(world, now_ms);
    Ok(())
}

crate::ecs::system::framework_system!(pub sim_input_system, SimInput);

struct SimRootFallback(Entity);

pub(crate) fn set_sim_root(world: &mut World, root: Entity) {
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

    /// `App` suppresses real interactive input while this is true.
    /// Host lifecycle requests continue to reach plugins.
    pub fn is_running(&self) -> bool {
        self.looping || self.cursor < self.entries.len()
    }
}

pub(crate) fn sim_timeline_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let clock_fn = world.resource::<crate::ecs::MonoClock>().map(|fc| fc.clock);
    let now_ms = clock_fn.map(|f| (f() / 1_000_000) as u32).unwrap_or(0);

    let (lw, lh) = world
        .resource::<crate::surface::DisplayInfo>()
        .map(|d| (d.width, d.height))
        .unwrap_or((128, 128));
    let root = world.resource::<SimRootFallback>().map(|f| f.0);

    // Tight scope releases the &mut Timeline borrow before entity_centre.
    let (entry, action_elapsed, action_started) = {
        let Some(tl) = world.resource_mut::<SimTimeline>() else {
            return Ok(());
        };
        if tl.entries.is_empty() {
            return Ok(());
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
            return Ok(());
        }

        let entry = tl.entries[tl.cursor];
        if elapsed < entry.start_ms {
            return Ok(());
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
        return Ok(());
    };

    match action {
        ResolvedAction::Tap(pt) => {
            if !action_started {
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.action_elapsed_ms = 0;
                }
                let event = InputEvent::PointerDown {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: pt.x,
                    y: pt.y,
                };
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= 50 {
                let event = InputEvent::PointerUp {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: pt.x,
                    y: pt.y,
                };
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(world, root, &event, now_ms, lw, lh);
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
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: from.x,
                    y: from.y,
                };
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= duration_ms as u32 && move_emitted {
                let event = InputEvent::PointerUp {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: to.x,
                    y: to.y,
                };
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else {
                // Catch-up branch (elapsed ≥ duration, no move yet) clamps t=1
                // so a sub-tick drag still emits one move before PointerUp.
                let t_raw = if action_elapsed >= duration_ms as u32 {
                    Fixed::ONE
                } else {
                    Fixed::from_ratio(action_elapsed as i32, duration_ms as i32)
                };
                let eased = ease(t_raw);
                let x = from.x + eased * (to.x - from.x);
                let y = from.y + eased * (to.y - from.y);
                let event = InputEvent::PointerMove {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x,
                    y,
                };
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.drag_move_emitted = true;
                }
                super::deliver_input(world, root, &event, now_ms, lw, lh);
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
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: from.x,
                    y: from.y,
                };
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= duration_ms as u32 {
                let event = InputEvent::PointerMove {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: to.x,
                    y: to.y,
                };
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else {
                let t = Fixed::from_ratio(action_elapsed as i32, duration_ms as i32);
                let eased = ease(t);
                let x = from.x + eased * (to.x - from.x);
                let y = from.y + eased * (to.y - from.y);
                let event = InputEvent::PointerMove {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x,
                    y,
                };
                super::deliver_input(world, root, &event, now_ms, lw, lh);
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
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Touch,
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
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            } else {
                let t = Fixed::from_ratio(action_elapsed as i32, duration_ms as i32);
                let eased = ease(t);
                let dist = from_dist + eased * (to_dist - from_dist);
                let (a, b) = two_fingers(dist);
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Touch,
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
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Touch,
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
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 2,
                        x: b.x,
                        y: b.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
            } else {
                let t = Fixed::from_ratio(action_elapsed as i32, duration_ms as i32);
                let eased = ease(t);
                let angle = from_angle + eased * (to_angle - from_angle);
                let (a, b) = two_fingers(angle);
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Touch,
                        id: 1,
                        x: a.x,
                        y: a.y,
                    },
                    now_ms,
                    lw,
                    lh,
                );
                super::deliver_input(
                    world,
                    root,
                    &InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Touch,
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
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.action_started = true;
                    tl.rotate_emitted = target;
                    if target >= total {
                        tl.cursor += 1;
                        tl.action_started = false;
                        tl.rotate_emitted = 0;
                    }
                }
                for _ in already..target {
                    let event = InputEvent::Rotary { id: 0, delta: sign };
                    super::deliver_input(world, root, &event, now_ms, lw, lh);
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
                    physical: None,
                    code: crate::input::event::input::KEY_ROTARY_PRESS,
                    pressed: true,
                    modifiers: Some(Default::default()),
                    repeat: Some(false),
                };
                super::deliver_input(world, root, &event, now_ms, lw, lh);
            } else if action_elapsed >= 50 {
                let event = InputEvent::Key {
                    physical: None,
                    code: crate::input::event::input::KEY_ROTARY_PRESS,
                    pressed: false,
                    modifiers: Some(Default::default()),
                    repeat: Some(false),
                };
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                    tl.drag_move_emitted = false;
                    tl.rotate_emitted = 0;
                }
                super::deliver_input(world, root, &event, now_ms, lw, lh);
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

    super::dispatch_pending_gestures(world, now_ms);
    Ok(())
}

crate::ecs::system::framework_system!(pub sim_timeline_system, SimInput);

#[cfg(test)]
#[path = "../../../tests/unit/input/event/sim/tests.rs"]
mod tests;
