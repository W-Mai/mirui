//! Widget binding policy for the reactive queues.

use crate::core::reactive::{BindingRole, register_entity_effect, schedule, with_world};
use crate::ecs::{Entity, World};

use super::dirty::{Dirty, VisualDirty};
use super::retained;

fn effect_tree_revision(world: &World) -> (u64, u64) {
    (
        world.component_revision::<super::Parent>(),
        world.component_revision::<super::Children>(),
    )
}

pub(crate) fn order_effects(world: &World, remaining: usize) {
    if remaining < 2 {
        return;
    }
    let retained = world.component_count(core::any::TypeId::of::<retained::RetainedEffects>()) > 0;
    if !retained
        && world.component_count(core::any::TypeId::of::<super::branch::CachedBranchVisibility>())
            == 0
    {
        return;
    }
    #[cfg(feature = "perf")]
    crate::trace_span!("reactive.order");
    let ancestry_depth = retained::ancestry_depth(world);
    schedule::sort_effect_queue(world.id(), remaining, retained, ancestry_depth);
}

/// Drain queued reactive work at input delivery and frame boundaries:
/// run pending effects, then publish their widget invalidations. Only live
/// entities are eligible for effects and invalidations.
pub fn flush_signal_dirty(world: &mut World) {
    schedule::flush(
        world,
        effect_tree_revision,
        order_effects,
        |world, entity, role| {
            retained::enabled(world, entity)
                && (role != BindingRole::Display || retained::display_enabled(world, entity))
        },
        |world, entity| {
            world.insert(entity, Dirty);
        },
        |world, entity| {
            world.insert(entity, VisualDirty::New);
        },
    );
}

/// An effect owned by the runtime and tied to a widget entity: it re-runs on
/// dependency change and lives until the widget is despawned (cleanup later),
/// not until a returned handle drops. Hence no handle is returned — dropping
/// one would immediately unregister the effect.
#[cfg_attr(not(test), allow(dead_code))]
pub fn effect_with_widget(entity: Entity, f: impl Fn() + 'static) {
    effect_with_role(entity, BindingRole::Ordinary, f);
}

#[doc(hidden)]
pub fn effect_with_role(entity: Entity, role: BindingRole, f: impl Fn() + 'static) {
    if role != BindingRole::Ordinary {
        with_world(|world| {
            world.watch_component_revision::<super::Parent>();
            world.watch_component_revision::<super::Children>();
        });
    }
    register_entity_effect(entity, role, f);
}

pub(crate) mod widget_binding {
    use crate::core::reactive::with_world_scope;

    use super::{BindingRole, Entity, World, effect_with_role};

    pub struct PreparedWidgetEffect<F> {
        entity: Entity,
        run: F,
        role: BindingRole,
    }

    pub fn prepare_widget_effect<F: Fn() + 'static>(
        entity: Entity,
        role: BindingRole,
        run: F,
    ) -> PreparedWidgetEffect<F> {
        PreparedWidgetEffect { entity, run, role }
    }

    impl<F: Fn() + 'static> PreparedWidgetEffect<F> {
        pub fn install_once(self, world: &mut World) {
            with_world_scope(world, self.run);
        }

        pub fn install(self, world: &mut World) {
            with_world_scope(world, || effect_with_role(self.entity, self.role, self.run));
        }
    }
}
