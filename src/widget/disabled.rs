use crate::ecs::{Entity, World};
use crate::types::Opa;
use crate::widget::Parent;
use crate::widget::dirty::Dirty;
use crate::widget::visibility::Hidden;

use super::Style;

/// Marker that disables interaction on the entity and its
/// descendants. Layout and render still run, but pointer / key
/// events are swallowed at dispatch entry, focus traversal skips,
/// and visuals dim through `Style.disabled_alpha`.
///
/// Walk semantics mirror `Hidden`: an ancestor carrying `Disabled`
/// disables the whole subtree. Toggle by inserting / removing.
pub struct Disabled;

/// Hardcoded 38% × 255 ≈ 97. Tracks Material 3's disabled-state alpha.
const DISABLED_ALPHA: Opa = 97;

// Hidden short-circuits: the entity is already invisible, so writing
// `disabled_alpha` would only dirty-trash a hidden subtree.
fn entity_is_disabled(world: &World, entity: Entity) -> bool {
    let mut cur = Some(entity);
    while let Some(e) = cur {
        if world.get::<Hidden>(e).is_some() {
            return false;
        }
        if world.get::<Disabled>(e).is_some() {
            return true;
        }
        cur = world.get::<Parent>(e).map(|p| p.0);
    }
    false
}

#[crate::system(order = ANIMATION)]
pub fn disabled_visual_system(world: &mut World) {
    let entities: alloc::vec::Vec<Entity> = world.query::<Style>().collect();
    for entity in entities {
        let want = if entity_is_disabled(world, entity) {
            Some(DISABLED_ALPHA)
        } else {
            None
        };
        let cur = world.get::<Style>(entity).and_then(|s| s.disabled_alpha);
        if cur != want {
            if let Some(style) = world.get_mut::<Style>(entity) {
                style.disabled_alpha = want;
            }
            world.insert(entity, Dirty);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_child(world: &mut World, parent: Entity) -> Entity {
        let child = world.spawn();
        world.insert(child, Style::default());
        world.insert(child, Parent(parent));
        child
    }

    fn make_root(world: &mut World) -> Entity {
        let root = world.spawn();
        world.insert(root, Style::default());
        root
    }

    #[test]
    fn marks_self_disabled_alpha() {
        let mut world = World::new();
        let e = make_root(&mut world);
        world.insert(e, Disabled);
        disabled_visual_system(&mut world);
        assert_eq!(
            world.get::<Style>(e).unwrap().disabled_alpha,
            Some(DISABLED_ALPHA)
        );
    }

    #[test]
    fn descendant_inherits_disabled_alpha() {
        let mut world = World::new();
        let parent = make_root(&mut world);
        let child = make_child(&mut world, parent);
        world.insert(parent, Disabled);
        disabled_visual_system(&mut world);
        assert_eq!(
            world.get::<Style>(child).unwrap().disabled_alpha,
            Some(DISABLED_ALPHA)
        );
    }

    #[test]
    fn removing_disabled_clears_alpha() {
        let mut world = World::new();
        let e = make_root(&mut world);
        world.insert(e, Disabled);
        disabled_visual_system(&mut world);
        world.remove::<Disabled>(e);
        disabled_visual_system(&mut world);
        assert_eq!(world.get::<Style>(e).unwrap().disabled_alpha, None);
    }

    #[test]
    fn hidden_ancestor_skips_disabled_alpha() {
        let mut world = World::new();
        let parent = make_root(&mut world);
        let child = make_child(&mut world, parent);
        world.insert(parent, Hidden);
        world.insert(parent, Disabled);
        disabled_visual_system(&mut world);
        assert_eq!(world.get::<Style>(child).unwrap().disabled_alpha, None);
    }

    #[test]
    fn unrelated_entity_unaffected() {
        let mut world = World::new();
        let a = make_root(&mut world);
        let b = make_root(&mut world);
        world.insert(a, Disabled);
        disabled_visual_system(&mut world);
        assert_eq!(world.get::<Style>(b).unwrap().disabled_alpha, None);
    }
}
