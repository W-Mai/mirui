use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::picture::PictureModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: <PictureModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::{PersistenceError, PersistencePlugin};
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_atlas_restoration.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("atlas_restoration persistence unavailable: {error}");
            return;
        }
    };

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(storage)
        .bytes(
            "atlas_restoration/save",
            move || ModelHandle::read(&save_model, PictureModel::encode_snapshot),
            move |bytes| {
                let model =
                    PictureModel::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)?;
                restore_model.restore(model);
                Ok(())
            },
        )
        .autosave(1000);
    app.add_plugin(plugin);
}
