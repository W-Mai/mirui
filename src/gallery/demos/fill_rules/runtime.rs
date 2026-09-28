use super::composition::build_widgets;
use super::render::fill_rules_view;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(fill_rules_view());
    app.compose(parent, build_widgets);
}
