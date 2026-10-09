use super::composition::build_widgets;
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(64, 64);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}
