use super::*;
use crate::ecs::{SystemScheduler, World};
use crate::prelude::*;
use crate::ui::{Children, ComputedRect, IdMap, UiScope};

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ParticleBounds { w: 1, h: 1 });
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    let bounds = world.resource::<ParticleBounds>().unwrap();
    assert_eq!(
        (bounds.w, bounds.h),
        (DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32)
    );
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn bounds_system_initializes_the_resource_from_the_arena() {
    let mut world = World::new();
    let arena = world.spawn_empty();
    world.insert(arena, ParticleArena);
    world.insert(
        arena,
        ComputedRect(Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(320),
            h: Fixed::from_int(180),
        }),
    );

    let mut scheduler = SystemScheduler::new();
    scheduler.add(particle_bounds_system::system());
    scheduler.run_all(&mut world).unwrap();

    let bounds = world.resource::<ParticleBounds>().unwrap();
    assert_eq!((bounds.w, bounds.h), (320, 180));
}
