use super::composition::build_widgets;
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::Children;
use crate::ui::ComputedRect;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::render_system::update_layout;

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
fn labels_stay_inside_phone_stage() {
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
    let stage = rect("spatial_animation_stage");
    let header = rect("spatial_animation_header");
    assert!(header.x >= stage.x);
    assert!(header.x + header.w <= stage.x + stage.w);
}
