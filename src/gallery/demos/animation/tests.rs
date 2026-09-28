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
    drop(cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn phone_header_stays_inside_the_animation_stage() {
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
    let stage = rect("animation_stage");
    let header = rect("animation_header");
    assert!(header.x >= stage.x);
    assert!(header.x + header.w <= stage.x + stage.w);
}
