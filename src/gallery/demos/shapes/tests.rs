use super::composition::build_widgets;
use super::view::{Shapes, shapes_view};
use crate::ecs::World;
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
    world.insert_resource(MonoClock::new(|| 12_000_000));
    {
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
    }
    assert!(
        world
            .query::<Shapes>()
            .iter()
            .map(|(_, state)| state.start_ms)
            .eq([0_u32, 12])
    );
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}
