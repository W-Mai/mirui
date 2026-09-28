use crate::prelude::*;

use super::{build_widgets, diamond_view};

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(diamond_view());
    app.compose(parent, build_widgets);
}
