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
    let clock_fn = world
        .resource::<crate::anim::FrameClock>()
        .map(|fc| fc.clock);
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

#[derive(Clone, Copy)]
pub enum SimAction {
    Tap {
        x: Fixed,
        y: Fixed,
    },
    Drag {
        from_x: Fixed,
        from_y: Fixed,
        to_x: Fixed,
        to_y: Fixed,
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
                SimAction::Tap { .. } => 100,
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
    let clock_fn = world
        .resource::<crate::anim::FrameClock>()
        .map(|fc| fc.clock);
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
        SimAction::Tap { x, y } => {
            if !tl.action_started {
                tl.action_started = true;
                tl.action_elapsed_ms = 0;
                let event = InputEvent::PointerDown { id: 0, x, y };
                let hit = hit_test(world, root, x, y, lw, lh);
                if let Some(gs) = world.resource_mut::<GestureSystem>() {
                    gs.recognizer.update(&event, now_ms, hit, &mut gs.events);
                }
            } else if action_elapsed >= 50 {
                let event = InputEvent::PointerUp { id: 0, x, y };
                if let Some(gs) = world.resource_mut::<GestureSystem>() {
                    gs.recognizer.update(&event, now_ms, None, &mut gs.events);
                }
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                }
            }
        }
        SimAction::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            duration_ms,
            ease,
        } => {
            if !tl.action_started {
                tl.action_started = true;
                tl.action_elapsed_ms = 0;
                let event = InputEvent::PointerDown {
                    id: 0,
                    x: from_x,
                    y: from_y,
                };
                let hit = hit_test(world, root, from_x, from_y, lw, lh);
                if let Some(gs) = world.resource_mut::<GestureSystem>() {
                    gs.recognizer.update(&event, now_ms, hit, &mut gs.events);
                }
            } else if action_elapsed >= duration_ms as u32 {
                let event = InputEvent::PointerUp {
                    id: 0,
                    x: to_x,
                    y: to_y,
                };
                if let Some(gs) = world.resource_mut::<GestureSystem>() {
                    gs.recognizer.update(&event, now_ms, None, &mut gs.events);
                }
                if let Some(tl) = world.resource_mut::<SimTimeline>() {
                    tl.cursor += 1;
                    tl.action_started = false;
                }
            } else {
                let t = Fixed::from_raw(
                    (action_elapsed as i32) * Fixed::ONE.raw() / (duration_ms as i32),
                );
                let eased = ease(t);
                let x = from_x + eased * (to_x - from_x);
                let y = from_y + eased * (to_y - from_y);
                let event = InputEvent::PointerMove { id: 0, x, y };
                if let Some(gs) = world.resource_mut::<GestureSystem>() {
                    gs.recognizer.update(&event, now_ms, None, &mut gs.events);
                }
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
