use crate::gallery::play::picture::PictureModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_atlas_restoration.bin"))
        .bytes(
            "atlas_restoration/save",
            |world| {
                world
                    .resource::<PictureModel>()
                    .map(PictureModel::encode_vec)
            },
            |world, bytes| {
                if let Ok(model) = PictureModel::decode(bytes) {
                    world.insert_resource(model);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
