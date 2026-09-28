use super::{Card, build_widgets};
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(ui!(compose Card));
    app.compose(parent, build_widgets);
}
