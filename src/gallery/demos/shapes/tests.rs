use super::composition::build_widgets;
use super::view::shapes_view;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::view::ViewRegistry;

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(shapes_view());
    world.insert_resource(reg);
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
