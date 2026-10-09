use super::composition::build_widgets;
use super::motion::spring_system;
use crate::ecs::{SystemScheduler, World};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::Children;
use crate::ui::ComputedRect;
use crate::ui::render_system::update_layout;

fn run_spring_motion(world: &mut World) {
    let mut scheduler = SystemScheduler::new();
    scheduler.add(spring_system::system());
    scheduler.run_all(world).unwrap();
}

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(320, 568);
    let parent = app.spawn_root().layout(LayoutStyle::default()).id();
    app.compose(parent, build_widgets());
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn labels_stay_inside_phone_stage() {
    let mut app = App::headless(320, 568);
    let parent = app.spawn_root().layout(LayoutStyle::default()).id();
    app.compose(parent, build_widgets());
    let world = &mut app.world;

    update_layout(world, parent, &Viewport::new(320, 568, Fixed::ONE));
    let rect = |id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let stage = rect("spatial_animation_stage");
    let header = rect("spatial_animation_header");
    assert!(header.x >= stage.x);
    assert!(header.x + header.w <= stage.x + stage.w);
}

#[test]
#[should_panic(expected = "requires resource")]
fn spring_motion_requires_frame_delta() {
    run_spring_motion(&mut World::new());
}
