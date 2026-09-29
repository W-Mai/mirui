use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::storage::{ReplayKind, TidalReplayLog, replay_tide};
use crate::gallery::play::tidal::TideModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: <TideModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_tidal_atlas.bin"))
        .bytes(
            "tidal_atlas/replay",
            move |_world| Some(ModelHandle::read(&save_model, TideModel::encode_replay)),
            move |_world, bytes| {
                let Ok(log) = TidalReplayLog::decode(bytes, ReplayKind::Tidal) else {
                    return;
                };
                let Ok(restored) = replay_tide(&log) else {
                    return;
                };
                restore_model.restore_replay(restored);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
