use super::catalog::icons;
use super::composition::build_widgets;
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::Children;
use crate::ui::render_system::update_layout;
use crate::ui::{ComputedRect, IdMap, UiScope};

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    assert!(
        world
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
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);

    update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
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
