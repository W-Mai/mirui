use super::{SettingsModel, build_widgets};
use crate::prelude::*;

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(crate::app::plugins::StdInstantClockPlugin);
    #[cfg(feature = "persistence")]
    match crate::gallery::play::storage::gallery_storage("mirui_reader_settings.bin") {
        Ok(storage) => {
            app.add_plugin(
                crate::core::persistence::PersistencePlugin::new(storage).autosave(1000),
            );
        }
        Err(error) => crate::warn!("settings persistence unavailable: {error}"),
    }
    let settings = app.add_model(SettingsModel::default()).finish();
    app.compose(parent, build_widgets(settings));
}
