use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::twin::{SAVE_LEN, TwinModel};
use crate::prelude::{App, RendererFactory, Surface};

pub(super) struct TwinArchive;

impl ModelCodec for TwinArchive {
    type Model = TwinModel;
    type Prepared = TwinModel;

    fn encode(
        &self,
        model: &TwinModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<(), PersistenceError> {
        output.clear();
        output
            .try_reserve_exact(SAVE_LEN)
            .map_err(|_| PersistenceError::Capacity)?;
        output.resize(SAVE_LEN, 0);
        model
            .encode_into(output)
            .map_err(|_| PersistenceError::Encoding)?;
        Ok(())
    }

    fn prepare(&self, _: &TwinModel, bytes: &[u8]) -> Result<TwinModel, PersistenceError> {
        TwinModel::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut TwinModel, prepared: TwinModel) {
        *model = prepared;
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_twin_beacons.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("twin_beacons persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(TwinArchive)
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
    fn codec_restores_registered_state_and_reuses_encoding_storage() {
        let mut source = TwinModel::default();
        source.move_direction(source.level().solution(0).unwrap());
        let mut bytes = Vec::new();
        TwinArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(&bytes[..4], b"TWB1");
        assert_eq!(
            tracked_allocations(|| TwinArchive.encode(&source, &mut bytes).unwrap()),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("twin_beacons/save", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(TwinArchive));
        let model = app.add_model(TwinModel::default()).finish();
        assert_eq!(model.steps(), 1);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.restart();
        assert_eq!(model.steps(), 0);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.steps(), 1);
        model.restart();
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.steps(), 0);
    }

    #[test]
    fn invalid_archive_preserves_initial_data_and_blocks_automatic_overwrite() {
        let mut storage = MemoryStorage::new();
        storage.write("twin_beacons/save", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(TwinArchive));
        let model = app.add_model(TwinModel::default()).finish();
        assert_eq!(model.steps(), 0);
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
