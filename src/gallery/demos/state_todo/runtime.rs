use super::composition::build_widgets;
use super::model::TodoModel;
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    let todo = app.add_model(TodoModel::default());
    app.compose(parent, |cx| build_widgets(cx, todo));
}
