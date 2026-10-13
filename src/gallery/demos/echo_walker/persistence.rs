use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::echo::EchoModel;
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) struct EchoArchive;

impl ModelCodec for EchoArchive {
    type Model = EchoModel;
    type Prepared = EchoModel;
    fn encode(
        &self,
        model: &EchoModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        model.encode_replay(output).map(|()| EncodeOutcome::Encoded)
    }
    fn prepare(&self, _: &EchoModel, bytes: &[u8]) -> Result<EchoModel, PersistenceError> {
        let log = EchoReplayLog::decode(bytes, ReplayKind::Echo)
            .map_err(|_| PersistenceError::InvalidArchive)?;
        replay_echo(&log).map_err(|_| PersistenceError::InvalidArchive)
    }
    fn apply(&self, model: &mut EchoModel, prepared: EchoModel) {
        *model = prepared;
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_echo_walker.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("echo_walker persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(EchoArchive)
        .autosave(1000);
    app.add_plugin(plugin);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        persistence::PersistenceStatus,
        storage::{MemoryStorage, Storage},
    };
    use crate::gallery::play::echo::Direction;
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    #[test]
    fn replay_codec_restores_before_registration_and_uses_root_save_candidates() {
        let mut source = EchoModel::default();
        source.step(Direction::Wait);
        let mut bytes = Vec::new();
        EchoArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(&bytes[..4], b"MAH1");
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    EchoArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("echo_walker/replay", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(EchoArchive));
        let model = app.add_model(EchoModel::default()).finish();
        assert_eq!(model.tick(), 1);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.step(Direction::Wait);
        assert_eq!(model.tick(), 2);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.tick(), 1);
        model.step(Direction::Wait);
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.tick(), 2);
    }

    #[test]
    fn invalid_replay_protects_the_archive_without_mutating_the_initial_game() {
        let mut storage = MemoryStorage::new();
        storage.write("echo_walker/replay", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(EchoArchive));
        let model = app.add_model(EchoModel::default()).finish();
        assert_eq!(model.tick(), 0);
        assert_eq!(
            app.persistence().status(&model),
            PersistenceStatus::Failed {
                error: PersistenceError::InvalidArchive,
                protected: true
            }
        );
        assert_eq!(
            app.persistence().save_all(),
            Err(PersistenceError::InvalidArchive)
        );
    }
}
