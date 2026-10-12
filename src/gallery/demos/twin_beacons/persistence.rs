use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, game: <TwinModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::{PersistenceError, PersistencePlugin};
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_twin_beacons.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("twin_beacons persistence unavailable: {error}");
            return;
        }
    };

    let save_game = game.clone();
    let restore_game = game;
    let plugin = PersistencePlugin::new(storage)
        .bytes(
            "twin_beacons/save",
            move || Some(ModelHandle::read(&save_game, TwinModel::encode_vec)),
            move |bytes| {
                let restored =
                    TwinModel::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)?;
                restore_game.restore(restored);
                Ok(())
            },
        )
        .autosave(1000);
    app.add_plugin(plugin);
}
