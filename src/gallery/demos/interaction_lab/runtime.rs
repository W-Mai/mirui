use super::composition::build_widgets;
use super::state::InteractionModel;
use crate::prelude::plugin::InputFeedbackPlugin;
use crate::prelude::*;

pub(super) fn install<B, F>(
    app: &mut App<B, F>,
    parent: Entity,
) -> <InteractionModel as crate::core::model::Model>::Handle
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(app);
    let model = app.add_model(InteractionModel::default()).finish();
    app.add_plugin(InputFeedbackPlugin::new());
    app.compose(parent, build_widgets(model.clone()));
    model
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let _model = install(app, parent);
}
