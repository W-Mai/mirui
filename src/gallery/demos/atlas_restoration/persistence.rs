use crate::core::model::ModelHandle;
use crate::gallery::play::picture::{PictureModel, PictureModelHandle};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: PictureModelHandle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_atlas_restoration.bin"))
        .bytes(
            "atlas_restoration/save",
            move |_world| Some(ModelHandle::read(&save_model, PictureModel::encode_vec)),
            move |_world, bytes| {
                if let Ok(model) = PictureModel::decode(bytes) {
                    restore_model.restore(model);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
