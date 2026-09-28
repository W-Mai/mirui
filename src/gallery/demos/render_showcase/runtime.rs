use super::composition::build_widgets;
use super::render::showcase_view;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(crate::gallery::SceneReplayWorkspacePlugin);
    app.with_widget(showcase_view());
    app.compose(parent, build_widgets);
}
