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
    #[cfg(feature = "persistence")]
    match crate::gallery::play::storage::gallery_storage("mirui_state_todo.bin") {
        Ok(storage) => {
            app.add_plugin(
                crate::core::persistence::PersistencePlugin::new(storage).autosave(2000),
            );
        }
        Err(error) => crate::warn!("todo persistence unavailable: {error}"),
    }
    let todo = app.add_model(TodoModel::default()).finish();
    app.compose(parent, build_widgets(todo));
}
