use super::composition::build_widgets;
use super::view::{Shapes, shapes_view};
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets().with_widget(shapes_view());
    let parent = app.spawn_root().id();
    assert!(app.world.resource::<MonoClock>().is_none());
    app.compose(parent, build_widgets());
    app.add_resource(MonoClock::new(|| 12_000_000));
    app.compose(parent, build_widgets());
    let world = &app.world;
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
