use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::echo::EchoModel;
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: <EchoModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_echo_walker.bin"))
        .bytes(
            "echo_walker/replay",
            move |_world| Some(ModelHandle::read(&save_model, EchoModel::encode_replay)),
            move |_world, bytes| {
                let Ok(log) = EchoReplayLog::decode(bytes, ReplayKind::Echo) else {
                    return;
                };
                let Ok(model) = replay_echo(&log) else {
                    return;
                };
                restore_model.restore_replay(model);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
