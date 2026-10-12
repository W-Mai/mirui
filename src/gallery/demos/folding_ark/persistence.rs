use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::fold::FoldModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, game: <FoldModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::{PersistenceError, PersistencePlugin};
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_folding_ark.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("folding_ark persistence unavailable: {error}");
            return;
        }
    };

    let save_game = game.clone();
    let restore_game = game;
    let plugin = PersistencePlugin::new(storage)
        .bytes(
            "folding_ark/save",
            move || Some(ModelHandle::read(&save_game, FoldModel::encode_vec)),
            move |bytes| {
                let restored =
                    FoldModel::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)?;
                restore_game.restore(restored);
                Ok(())
            },
        )
        .autosave(1000);
    app.add_plugin(plugin);
}
