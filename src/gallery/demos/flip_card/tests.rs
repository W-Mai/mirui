use super::animation::flip_system;
use super::composition::build_widgets;
use super::state::FlipCard;
use crate::ecs::{DeltaTimeMs, SystemScheduler};
use crate::prelude::*;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{Children, IdMap, UiScope};

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
    build_widgets(&mut cx);
    drop(cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn card_rotation_uses_frame_delta() {
    let mut world = World::new();
    world.insert_resource(Theme::dark());
    world.insert_resource(DeltaTimeMs(20));
    let root = world.spawn_empty();
    let card = world.spawn_empty();
    world.insert(
        card,
        FlipCard {
            angle_deg: Fixed::ZERO,
            speed_deg_per_second: Fixed::from_int(60),
            front_color: ColorToken::Primary,
            back_color: ColorToken::Error,
            root,
        },
    );
    world.insert(card, Style::default());

    run_flip(&mut world);
    assert_eq!(
        world.get::<FlipCard>(card).unwrap().angle_deg,
        Fixed::from_int(60) * Fixed::from_ratio(20, 1_000)
    );
    assert!(world.get::<WidgetTransform3D>(card).is_some());
}

#[test]
#[should_panic(expected = "requires resource")]
fn card_rotation_requires_frame_delta() {
    run_flip(&mut World::new());
}
