use super::composition::build_widgets;
use crate::core::i18n::I18n;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(crate::render::font::default_font_manager());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    let column = world
        .get::<Children>(parent)
        .and_then(|c| c.0.first().copied())
        .expect("Column spawned under parent");
    let column_children = world
        .get::<Children>(column)
        .expect("Column has children")
        .0
        .len();
    assert_eq!(column_children, 4, "3 labels + 1 toggle button");
    assert!(world.resource::<I18n>().is_some(), "I18n resource inserted");
}
