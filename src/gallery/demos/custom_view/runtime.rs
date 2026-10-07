use crate::prelude::*;

use super::build_widgets;
use super::view::diamond_render;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_event_capacity(
        EventCapacity::new().commands(CommandCapacity::new().insert::<super::view::Diamond>(1)),
    );
    app.with_widget(diamond_render::view());
    app.compose(parent, build_widgets);
}
