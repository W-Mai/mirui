use super::animation::flip_system;
use super::composition::build_widgets;
use super::state::Page;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;

fn run_flip(world: &mut World) {
    let mut scheduler = SystemScheduler::new();
    scheduler.add(flip_system::system());
    scheduler.run_all(world).unwrap();
}

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets()(&mut cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn page_motion_uses_frame_delta_and_reflects_at_bounds() {
    let mut world = World::new();
    world.insert_resource(DeltaTimeMs(20));
    let page = world.spawn_empty();
    world.insert(
        page,
        Page {
            angle_deg: Fixed::ZERO,
            speed_deg_per_second: Fixed::from_int(30),
        },
    );

    run_flip(&mut world);
    assert_eq!(
        world.get::<Page>(page).unwrap().angle_deg,
        Fixed::from_int(30) * Fixed::from_ratio(20, 1_000)
    );

    let state = world.get_mut::<Page>(page).unwrap();
    state.angle_deg = Fixed::from_int(120);
    run_flip(&mut world);
    let state = world.get::<Page>(page).unwrap();
    assert!(state.angle_deg < Fixed::from_int(120));
    assert!(state.speed_deg_per_second < Fixed::ZERO);
}

#[test]
#[should_panic(expected = "requires resource")]
fn page_motion_requires_frame_delta() {
    run_flip(&mut World::new());
}
