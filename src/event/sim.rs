use alloc::vec::Vec;

use crate::ecs::{Entity, World};

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
