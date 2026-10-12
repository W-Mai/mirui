use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::echo::EchoModel;
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>, model: <EchoModel as Model>::Handle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::{PersistenceError, PersistencePlugin};
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_echo_walker.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("echo_walker persistence unavailable: {error}");
            return;
        }
    };

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(storage)
        .bytes(
            "echo_walker/replay",
            move || Some(ModelHandle::read(&save_model, EchoModel::encode_replay)),
            move |bytes| {
                let log = EchoReplayLog::decode(bytes, ReplayKind::Echo)
                    .map_err(|_| PersistenceError::InvalidArchive)?;
                let model = replay_echo(&log).map_err(|_| PersistenceError::InvalidArchive)?;
                restore_model.restore_replay(model);
                Ok(())
            },
        )
        .autosave(1000);
    app.add_plugin(plugin);
}
