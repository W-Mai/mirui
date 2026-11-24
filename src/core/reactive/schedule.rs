//! Queue draining and ordering for effects and widget invalidations.

use alloc::collections::VecDeque;
use alloc::rc::Rc;

use super::{
    BindingRole, PaintRequestCondition, PaintWake, PendingEffect, ReactiveOwnerCleanup,
    VisualInvalidation, WorldGuard, reclaim_dead_effects, register_graph_owner,
    release_graph_owner, run_effect, with_reactive,
};
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};

// An effect re-running may set signals that enqueue more effects/widgets in the
// same frame; loop until both queues settle. The cap stops a runaway cycle from
// hanging the frame.
const FLUSH_MAX_PASSES: u32 = 32;

fn pop_widget_for(
    queue: &mut VecDeque<(Option<WorldId>, Entity)>,
    world_id: WorldId,
) -> Option<Entity> {
    let (owner, entity) = queue.pop_front().expect("queued widget");
    if owner.is_none_or(|owner| owner == world_id) {
        Some(entity)
    } else {
        queue.push_back((owner, entity));
        None
    }
}

fn pop_visual_widget_for(
    queue: &mut VecDeque<VisualInvalidation>,
    world_id: WorldId,
) -> Option<Entity> {
    let entry = queue.pop_front().expect("queued visual widget");
    if entry.pending.is_none()
        && entry.condition != PaintRequestCondition::OnResourceWake
        && entry.owner.is_none_or(|owner| owner == world_id)
    {
        Some(entry.entity)
    } else {
        queue.push_back(entry);
        None
    }
}

pub(crate) fn sort_effect_queue(
    world_id: WorldId,
    remaining: usize,
    order_ordinary_by_depth: bool,
    ancestry_depth: impl Fn(Entity) -> usize,
) {
    with_reactive(|runtime| {
        let count = remaining.min(runtime.dirty_effects.len());
        for (index, entry) in runtime.dirty_effects.iter_mut().take(count).enumerate() {
            entry.order = runtime.effects.get(&entry.id).map_or(0, |effect| {
                let effect = effect.borrow();
                let foreign = effect.owner_world.is_some_and(|owner| owner != world_id);
                let ordinary = effect.role == BindingRole::Ordinary;
                let (lane, depth) = if foreign {
                    // Entity identities only resolve within their own World.
                    (3, 0)
                } else if ordinary && !order_ordinary_by_depth {
                    (1, index)
                } else {
                    (
                        if ordinary { 1 } else { 2 },
                        effect.owner_entity.map_or(0, &ancestry_depth),
                    )
                };
                // The 32-bit identity space bounds both depth and the queue ordinal.
                // Keep the cycle sentinel above every finite depth on all targets.
                let depth = if depth == usize::MAX {
                    1u64 << 32
                } else {
                    depth as u64
                };
                // Zero sorts missing effects first. Each live lane has 33 depth
                // bits and two role bits; the full ID breaks remaining ties.
                (lane << 35) | (depth << 2) | u64::from(effect.role as u8)
            });
        }
        runtime.dirty_effects.make_contiguous()[..count]
            .sort_unstable_by_key(|entry| (entry.order, entry.id));
    });
}

#[cfg_attr(feature = "perf", crate::trace_fn("reactive.flush"))]
pub(crate) fn flush(
    world: &mut World,
    effect_tree_revision: impl Fn(&World) -> (u64, u64),
    order_effects: impl Fn(&World, usize),
    effect_enabled: impl Fn(&World, Entity, BindingRole) -> bool,
    mark_dirty: impl Fn(&mut World, Entity),
    mark_visual_dirty: impl Fn(&mut World, Entity),
) {
    let resource_ready = world
        .resource::<Rc<PaintWake>>()
        .is_some_and(|wake| wake.0.take_ready());
    with_reactive(|r| {
        if !r.dirty_visual_widgets.is_empty() {
            r.dirty_visual_widgets.retain_mut(|entry| {
                if entry.owner == Some(world.id())
                    && entry.condition == PaintRequestCondition::OnResourceWake
                {
                    if !world.is_alive(entry.entity) {
                        return false;
                    }
                    if resource_ready {
                        entry.condition = PaintRequestCondition::Always;
                    }
                }
                true
            });
        }
    });
    // Preserve cold preparation even when no callback needs a structural window.
    world.register_drop_hook::<ReactiveOwnerCleanup>(release_graph_owner);
    register_graph_owner(world.id());
    reclaim_dead_effects(world);
    // Retiring captures may enqueue work, so inspect the queues after cleanup.
    if with_reactive(|r| {
        r.dirty_effects.is_empty()
            && r.dirty_widgets.is_empty()
            && r.dirty_visual_widgets.is_empty()
    }) {
        return;
    }
    let _guard = WorldGuard::enter(world);
    let world_id = world.id();
    for _ in 0..FLUSH_MAX_PASSES {
        let mut ran_effect = false;
        {
            #[cfg(feature = "perf")]
            crate::trace_span!("reactive.effects");
            let pending_effects = with_reactive(|r| r.dirty_effects.len());
            if pending_effects > 1 {
                order_effects(world, pending_effects);
            }
            let mut tree_revision = effect_tree_revision(world);
            for index in 0..pending_effects {
                let revision = effect_tree_revision(world);
                if revision != tree_revision {
                    order_effects(world, pending_effects - index);
                    tree_revision = revision;
                }
                let effect = with_reactive(|r| {
                    let entry = r.dirty_effects.pop_front()?;
                    let id = entry.id;
                    let effect = r.effects.get(&id)?.borrow();
                    if effect.owner_world.is_some_and(|owner| owner != world_id) {
                        r.dirty_effects.push_back(entry);
                        return None;
                    }
                    effect.queued.set(false);
                    Some((id, effect.owner_entity, effect.role))
                });
                if let Some((effect, owner, role)) = effect {
                    if owner.is_some_and(|entity| {
                        !world.is_alive(entity) || !effect_enabled(world, entity, role)
                    }) {
                        continue;
                    }
                    run_effect(effect);
                    ran_effect = true;
                }
            }
        }
        let mut marked_widget = false;
        let mut marked_visual_widget = false;
        {
            #[cfg(feature = "perf")]
            crate::trace_span!("reactive.mark_dirty");
            let pending_widgets = with_reactive(|r| r.dirty_widgets.len());
            for _ in 0..pending_widgets {
                let entity = with_reactive(|r| pop_widget_for(&mut r.dirty_widgets, world_id));
                if let Some(entity) = entity {
                    marked_widget = true;
                    if world.is_alive(entity) {
                        mark_dirty(world, entity);
                    }
                }
            }
            let pending_visual_widgets = with_reactive(|r| r.dirty_visual_widgets.len());
            for _ in 0..pending_visual_widgets {
                let entity =
                    with_reactive(|r| pop_visual_widget_for(&mut r.dirty_visual_widgets, world_id));
                if let Some(entity) = entity {
                    marked_visual_widget = true;
                    if world.is_alive(entity) {
                        mark_visual_dirty(world, entity);
                    }
                }
            }
        }
        if !ran_effect && !marked_widget && !marked_visual_widget {
            return;
        }
    }
    // cap reached = a cycle; debug panics, release stops instead of hanging
    debug_assert!(
        false,
        "reactive flush did not settle in {FLUSH_MAX_PASSES} passes (cycle?)"
    );
}

pub(crate) fn wake_entity_effects(
    world: &World,
    contains: impl Fn(Entity) -> bool,
    accepts: impl Fn(BindingRole) -> bool,
) {
    with_reactive(|runtime| {
        if let Some(effects) = runtime.effect_index.entity_effects(world.id()) {
            let queued = runtime.dirty_effects.len();
            let mut previous = None;
            let mut included = false;
            for (entity, id) in effects {
                if previous != Some(entity) {
                    previous = Some(entity);
                    included = contains(entity);
                }
                if included {
                    let effect = runtime.effects[&id].borrow();
                    if accepts(effect.role) {
                        PendingEffect::enqueue(&mut runtime.dirty_effects, id, &effect);
                    }
                }
            }
            if runtime.dirty_effects.len() - queued > 1 {
                // Wake order follows the registry even when entity IDs differ.
                runtime.dirty_effects.make_contiguous()[queued..]
                    .sort_unstable_by_key(|entry| entry.id);
            }
            return;
        }
        for (&id, effect) in &runtime.effects {
            let effect = effect.borrow();
            if effect.owner_world == Some(world.id())
                && accepts(effect.role)
                && effect.owner_entity.is_some_and(&contains)
            {
                PendingEffect::enqueue(&mut runtime.dirty_effects, id, &effect);
            }
        }
    });
}
