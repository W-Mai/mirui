#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::state::InteractionModel;
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub(super) fn install<B, F>(
    app: &mut App<B, F>,
    parent: Entity,
) -> <InteractionModel as crate::core::model::Model>::Handle
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_event_capacity(
        EventCapacity::new()
            .notifications::<crate::ui::widgets::switch::Toggled>(1)
            .notifications::<crate::ui::widgets::checkbox::Toggled>(1)
            .flush_limit(2),
    );
    crate::gallery::showcase_theme::install(app);
    let model = app.add_model(InteractionModel::default());
    app.add_plugin(InputFeedbackPlugin::new());
    app.compose(parent, |cx| build_widgets(cx, model.clone()));
    model
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let _model = install(app, parent);
}
