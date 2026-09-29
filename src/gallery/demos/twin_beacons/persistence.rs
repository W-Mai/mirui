use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, game: <TwinModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_game = game.clone();
    let restore_game = game;
    let mut saved_revision = save_game.persistence_revision();
    let plugin = PersistencePlugin::new(gallery_storage("mirui_twin_beacons.bin"))
        .bytes(
            "twin_beacons/save",
            move |_world| {
                let revision = save_game.persistence_revision();
                if revision == saved_revision {
                    return None;
                }
                let bytes = ModelHandle::read(&save_game, TwinModel::encode_vec);
                saved_revision = revision;
                Some(bytes)
            },
            move |_world, bytes| {
                if let Ok(restored) = TwinModel::decode(bytes) {
                    restore_game.restore(restored);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
