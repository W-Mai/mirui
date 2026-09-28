use alloc::vec::Vec;

use super::simulation::BODY_SIZE;
use super::state::LayoutOrigin;
use super::*;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, 128, 128, 3, Fixed::from_int(30));
    drop(cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn animation_systems_reuse_body_entities() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, 128, 128, 3, Fixed::from_int(30));
    drop(cx);

    let mut entities = Vec::new();
    world
        .query::<PhysicsBody>()
        .and::<Velocity>()
        .collect_into(&mut entities);
    assert_eq!(entities.len(), 3);
    let selected = entities[1];
    let initial_velocity = world.get::<Velocity>(selected).unwrap().vx;
    let scratch = world.resource_mut::<PhysicsScratch>().unwrap();
    scratch.entities = entities;
    let entities_ptr = scratch.entities.as_ptr();
    world.resource_mut::<KickPhase>().unwrap().0 = 39;

    kick_system(&mut world);
    assert_ne!(
        world.get::<Velocity>(selected).unwrap().vx,
        initial_velocity
    );
    let body = world.get_mut::<PhysicsBody>(selected).unwrap();
    body.x = Fixed::from_int(40);
    body.y = Fixed::from_int(50);
    sync_layout_system(&mut world);

    let origin = *world.get::<LayoutOrigin>(selected).unwrap();
    let transform = world
        .get::<crate::ui::widgets::WidgetTransform>(selected)
        .unwrap()
        .0;
    assert_eq!(
        transform,
        Transform::translate(
            Fixed::from_int(40 - BODY_SIZE / 2) - origin.x,
            Fixed::from_int(50 - BODY_SIZE / 2) - origin.y,
        )
    );
    assert!(
        world
            .get::<crate::ui::dirty::VisualDirty>(selected)
            .is_some()
    );
    assert_eq!(
        world
            .resource::<PhysicsScratch>()
            .unwrap()
            .entities
            .as_ptr(),
        entities_ptr
    );
}

#[test]
fn viewport_resize_recenters_bodies_and_orbit_guides() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, 480, 320, 3, Fixed::from_int(70));
    drop(cx);

    let body = world.query::<PhysicsBody>().iter().next().unwrap().0;
    let before = world.get::<PhysicsBody>(body).unwrap();
    let before = (before.x, before.y);
    WorldBounds::sync(&mut world, 320, 480);
    let after = world.get::<PhysicsBody>(body).unwrap();
    assert_eq!(after.x, before.0 - Fixed::from_int(80));
    assert_eq!(after.y, before.1 + Fixed::from_int(80));

    let outer = world
        .query::<OrbitRing>()
        .iter()
        .find(|(_, ring)| ring.diameter_percent == 72)
        .unwrap()
        .0;
    let layout = &world.get::<Style>(outer).unwrap().layout;
    assert_eq!(layout.width, Dimension::px(230));
    assert_eq!(layout.height, Dimension::px(230));
    assert_eq!(layout.left, Dimension::px(45));
    assert_eq!(layout.top, Dimension::px(125));
}
