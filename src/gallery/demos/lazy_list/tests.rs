use super::binding::row_binder;
use super::composition::build_widgets;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::widgets::{LazyListPool, Text};

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
    let list = world.find_by_id("lazy_list_view").unwrap();
    let row = world.get::<LazyListPool>(list).unwrap().items[0];
    assert!(!world.has::<Text>(row));
    let label = world.get::<Children>(row).unwrap().0[0];
    let paragraph = world.get::<Text>(label).unwrap().paragraph().clone();
    row_binder(&mut world, row, 42);
    assert_eq!(world.get::<Text>(label).unwrap().resolve(&world), "Row 42");
    assert_eq!(world.get::<Text>(label).unwrap().paragraph(), &paragraph);
    assert_eq!(
        world.get::<Style>(row).unwrap().bg_color,
        Some(ColorToken::Surface.into())
    );
}
