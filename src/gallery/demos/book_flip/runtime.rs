use super::animation::flip_system;
use super::composition::build_widgets;
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(flip_system::system());
    app.compose(parent, build_widgets);
}
