#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::model::CounterModel;
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    let counter = app.add_model(CounterModel::default());
    app.compose(parent, |cx| build_widgets(cx, counter));
}
