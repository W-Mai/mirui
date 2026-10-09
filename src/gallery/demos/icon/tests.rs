use super::catalog::icons;
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
fn icons_table_has_twenty_entries() {
    assert_eq!(icons().len(), 20);
}

#[test]
fn icon_grid_stays_inside_phone_shell() {
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
    let shell = rect("icon_gallery_shell");
    for id in ["icon_gallery_grid", "icon_gallery_motion"] {
        let child = rect(id);
        assert!(child.x >= shell.x);
        assert!(child.x + child.w <= shell.x + shell.w);
    }
}
