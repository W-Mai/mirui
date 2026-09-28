use super::composition::build_widgets;
use super::motion::{Spinner, spin_system};
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{Children, IdMap, UiScope};

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
fn image_motion_uses_frame_delta() {
    let mut world = World::new();
    world.insert_resource(DeltaTimeMs(20));
    let image = world.spawn_empty();
    world.insert(
        image,
        Spinner {
            angle: Fixed::ZERO,
            speed_deg_per_second: Fixed::from_int(180),
            bounce_phase: Fixed::ZERO,
        },
    );

    spin_system(&mut world);
    let spinner = world.get::<Spinner>(image).unwrap();
    let expected = Fixed::from_int(180) * Fixed::from_ratio(20, 1_000);
    assert_eq!(spinner.angle, expected);
    assert_eq!(spinner.bounce_phase, expected);
    assert!(world.get::<WidgetTransform3D>(image).is_some());
}
