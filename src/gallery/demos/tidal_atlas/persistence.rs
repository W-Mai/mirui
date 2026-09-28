use crate::gallery::play::storage::{ReplayKind, TidalReplayLog, replay_tide};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_tidal_atlas.bin"))
        .bytes(
            "tidal_atlas/replay",
            |world| {
                world
                    .resource::<TidalReplayLog>()
                    .map(|log| log.encode_vec())
            },
            |world, bytes| {
                let Ok(log) = TidalReplayLog::decode(bytes, ReplayKind::Tidal) else {
                    return;
                };
                let Ok(model) = replay_tide(&log) else {
                    return;
                };
                world.insert_resource(log);
                world.insert_resource(model);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
