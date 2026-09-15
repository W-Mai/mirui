use crate::types::Transform;

/// Per-entity 2D affine. Absent = identity, zero cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetTransform(pub Transform);

impl From<Transform> for WidgetTransform {
    fn from(t: Transform) -> Self {
        Self(t)
    }
}

/// Update an entity's 2D transform without invalidating flex or text layout.
pub fn set_transform(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    transform: Transform,
) {
    if world
        .get::<WidgetTransform>(entity)
        .is_some_and(|current| current.0 == transform)
    {
        return;
    }
    world.insert(entity, WidgetTransform(transform));
    world.insert(entity, crate::ui::dirty::VisualDirty);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::types::Fixed;
    use crate::ui::dirty::VisualDirty;

    #[test]
    fn visual_transform_marks_only_changed_values() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let transform = Transform::translate(Fixed::from_int(4), Fixed::from_int(3));
        set_transform(&mut world, entity, transform);
        assert_eq!(world.get::<WidgetTransform>(entity).unwrap().0, transform);
        assert!(world.get::<VisualDirty>(entity).is_some());

        world.remove::<VisualDirty>(entity);
        set_transform(&mut world, entity, transform);
        assert!(world.get::<VisualDirty>(entity).is_none());
    }
}
