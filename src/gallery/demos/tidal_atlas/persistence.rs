use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::storage::{ReplayKind, TidalReplayLog, replay_tide};
use crate::gallery::play::tidal::TideModel;
use crate::prelude::{App, RendererFactory, Surface};

pub(super) struct TideArchive;

impl ModelCodec for TideArchive {
    type Model = TideModel;
    type Prepared = TideModel;
    fn encode(
        &self,
        model: &TideModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        model.encode_replay(output).map(|()| EncodeOutcome::Encoded)
    }
    fn prepare(&self, _: &TideModel, bytes: &[u8]) -> Result<TideModel, PersistenceError> {
        let log = TidalReplayLog::decode(bytes, ReplayKind::Tidal)
            .map_err(|_| PersistenceError::InvalidArchive)?;
        replay_tide(&log).map_err(|_| PersistenceError::InvalidArchive)
    }
    fn apply(&self, model: &mut TideModel, prepared: TideModel) {
        *model = prepared;
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_tidal_atlas.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("tidal_atlas persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(TideArchive)
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
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    #[test]
    fn replay_codec_restores_before_registration_and_uses_root_save_candidates() {
        let mut source = TideModel::default();
        let index = (0..36).find(|index| source.valid(*index)).unwrap() as u8;
        source.place(index, 0);
        let mut bytes = Vec::new();
        TideArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(&bytes[..4], b"MAH1");
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    TideArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("tidal_atlas/replay", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(TideArchive));
        let model = app.add_model(TideModel::default()).finish();
        assert_eq!(model.turn(), 1);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.undo();
        assert_eq!(model.turn(), 0);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.turn(), 1);
        model.undo();
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.turn(), 0);
    }

    #[test]
    fn invalid_replay_protects_the_archive_without_mutating_the_initial_game() {
        let mut storage = MemoryStorage::new();
        storage.write("tidal_atlas/replay", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(TideArchive));
        let model = app.add_model(TideModel::default()).finish();
        assert_eq!(model.turn(), 0);
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
