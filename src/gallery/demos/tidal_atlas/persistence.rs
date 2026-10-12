use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::storage::{ReplayKind, TidalReplayLog, replay_tide};
use crate::gallery::play::tidal::TideModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: <TideModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::{PersistenceError, PersistencePlugin};
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_tidal_atlas.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("tidal_atlas persistence unavailable: {error}");
            return;
        }
    };

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(storage)
        .bytes(
            "tidal_atlas/replay",
            move || Some(ModelHandle::read(&save_model, TideModel::encode_replay)),
            move |bytes| {
                let log = TidalReplayLog::decode(bytes, ReplayKind::Tidal)
                    .map_err(|_| PersistenceError::InvalidArchive)?;
                let restored = replay_tide(&log).map_err(|_| PersistenceError::InvalidArchive)?;
                restore_model.restore_replay(restored);
                Ok(())
            },
        )
        .autosave(1000);
    app.add_plugin(plugin);
}
