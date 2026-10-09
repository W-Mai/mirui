use super::composition::build_widgets;
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::Children;
use crate::ui::ComputedRect;
use crate::ui::render_system::update_layout;

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
fn phone_header_stays_inside_the_animation_stage() {
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
    let stage = rect("animation_stage");
    let header = rect("animation_header");
    assert!(header.x >= stage.x);
    assert!(header.x + header.w <= stage.x + stage.w);
}
