use alloc::vec::Vec;

use super::simulation::BODY_SIZE;
use super::state::LayoutOrigin;
use super::*;
use crate::ecs::SystemScheduler;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::Children;

#[test]
fn physics_uses_fixed_steps_and_integer_equilibrium_distance() {
    fn clock() -> u64 {
        11_000_000
    }

    let mut world = crate::ecs::World::new();
    world.insert_resource(MonoClock::new(clock));
    world.insert_resource(PhysicsTime {
        last_tick_ms: 0,
        accumulator_ms: 0,
    });
    world.insert_resource(WorldBounds { w: 128, h: 128 });
    world.insert_resource(SpringLength(Fixed::from_int(5)));
    world.insert_resource(PhysicsScratch::default());
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    for (entity, x, y, vx, vy) in [(first, 30, 30, 1, 0), (second, 33, 34, 0, 1)] {
        world.insert(
            entity,
            PhysicsBody {
                x: x.into(),
                y: y.into(),
            },
        );
        world.insert(
            entity,
            Velocity {
                vx: vx.into(),
                vy: vy.into(),
            },
        );
    }
    let mut scheduler = SystemScheduler::new();
    scheduler.add(physics_tick_system::system());
    scheduler.run_all(&mut world).unwrap();
    let positions = |world: &crate::ecs::World| {
        [first, second].map(|entity| {
            let body = world.get::<PhysicsBody>(entity).unwrap();
            (body.x, body.y)
        })
    };
    assert_eq!(
        positions(&world),
        [(31.into(), 30.into()), (33.into(), 35.into())]
    );
    assert_eq!(world.resource::<PhysicsTime>().unwrap().accumulator_ms, 0);
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(
        positions(&world),
        [(31.into(), 30.into()), (33.into(), 35.into())]
    );

    // sqrt(3*3 + 5*5) floors to the same equilibrium distance.
    world.get_mut::<PhysicsBody>(first).unwrap().x = 30.into();
    world.resource_mut::<PhysicsTime>().unwrap().last_tick_ms = 0;
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(
        positions(&world),
        [(31.into(), 30.into()), (33.into(), 36.into())]
    );
}

#[test]
fn kicks_only_record_the_selected_live_body() {
    use core::any::TypeId;
    let mut world = crate::ecs::World::new();
    let first = world.spawn_empty();
    let target = world.spawn_empty();
    for entity in [first, target] {
        world.insert(
            entity,
            Velocity {
                vx: Fixed::ZERO,
                vy: Fixed::ZERO,
            },
        );
    }
    world.insert_resource(KickPhase(39));
    world.insert_resource(PhysicsScratch {
        entities: alloc::vec![first, target],
        ..Default::default()
    });
    world.watch_component_type(TypeId::of::<Velocity>());
    world.drain_component_changes(|_, _| {});
    let mut scheduler = SystemScheduler::new();
    scheduler.add(kick_system::system());
    scheduler.run_all(&mut world).unwrap();
    let mut changed = Vec::new();
    world.drain_component_changes(|_, change| changed.push(change.entity));
    assert_eq!(changed, [target]);
    assert_eq!(world.get::<Velocity>(first).unwrap().vx, Fixed::ZERO);

    assert!(world.despawn(target));
    let replacement = world.spawn_empty();
    assert_eq!(replacement.id, target.id);
    world.insert(
        replacement,
        Velocity {
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
        },
    );
    world.drain_component_changes(|_, _| {});
    world.resource_mut::<KickPhase>().unwrap().0 = 39;
    scheduler.run_all(&mut world).unwrap();
    world.drain_component_changes(|_, _| panic!("stale target changed a component"));
    assert_eq!(world.get::<Velocity>(replacement).unwrap().vx, Fixed::ZERO);
}

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(128, 128);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(128, 128, 3, Fixed::from_int(30)));
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn animation_systems_reuse_body_entities() {
    let mut app = App::headless(128, 128);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(128, 128, 3, Fixed::from_int(30)));
    let world = &mut app.world;

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

    let mut scheduler = SystemScheduler::new();
    scheduler.add(kick_system::system());
    scheduler.add(sync_layout_system::system());
    scheduler.run_all(world).unwrap();
    assert_ne!(
        world.get::<Velocity>(selected).unwrap().vx,
        initial_velocity
    );
    let body = world.get_mut::<PhysicsBody>(selected).unwrap();
    body.x = Fixed::from_int(40);
    body.y = Fixed::from_int(50);
    scheduler.run_all(world).unwrap();

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
            .is_some_and(|dirty| dirty.transforms_subtree())
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
fn motion_preserves_pending_content_and_skips_unchanged_transforms() {
    use crate::ui::dirty::VisualDirty;
    let mut app = App::headless(128, 128);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(128, 128, 3, Fixed::from_int(30)));
    let world = &mut app.world;
    let selected = world.query::<PhysicsBody>().iter().next().unwrap().0;
    let mut scheduler = SystemScheduler::new();
    scheduler.add(sync_layout_system::system());
    scheduler.run_all(world).unwrap();
    world.remove::<VisualDirty>(selected);
    scheduler.run_all(world).unwrap();
    assert!(world.get::<VisualDirty>(selected).is_none());
    world.invalidate_visual(selected);
    world.get_mut::<PhysicsBody>(selected).unwrap().x += Fixed::ONE;
    scheduler.run_all(world).unwrap();
    assert!(matches!(
        world.get::<VisualDirty>(selected).copied(),
        Some(VisualDirty::NewTransform)
    ));
}

#[test]
fn viewport_resize_recenters_bodies_and_orbit_guides() {
    use crate::types::Viewport;
    use crate::ui::render_system::try_update_layout;

    fn stopped_clock() -> u64 {
        0
    }

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets(480, 320, 3, Fixed::from_int(70)));
    app.set_root(root);
    app.world.insert_resource(MonoClock::new(stopped_clock));

    let body = app.world.query::<PhysicsBody>().iter().next().unwrap().0;
    let before = app.world.get::<PhysicsBody>(body).unwrap();
    let before = (before.x, before.y);
    try_update_layout(&mut app.world, root, &Viewport::new(320, 480, Fixed::ONE)).unwrap();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(physics_tick_system::system());
    scheduler.add(sync_layout_system::system());
    scheduler.run_all(&mut app.world).unwrap();

    let after = app.world.get::<PhysicsBody>(body).unwrap();
    assert_eq!(after.x, before.0 - Fixed::from_int(80));
    assert_eq!(after.y, before.1 + Fixed::from_int(80));

    let outer = app
        .world
        .query::<OrbitRing>()
        .iter()
        .find(|(_, ring)| ring.diameter_percent == 72)
        .unwrap()
        .0;
    let layout = &app.world.get::<Style>(outer).unwrap().layout;
    assert_eq!(layout.width, Dimension::px(230));
    assert_eq!(layout.height, Dimension::px(230));
    assert_eq!(layout.left, Dimension::px(45));
    assert_eq!(layout.top, Dimension::px(125));
}
