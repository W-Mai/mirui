use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::factory::{FactoryModel, FactoryProgress, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// Keeps the stars of every order and the open order.
pub(super) struct FactoryArchive;

impl ModelCodec for FactoryArchive {
    type Model = FactoryModel;
    type Prepared = FactoryProgress;

    fn encode(
        &self,
        model: &FactoryModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        output.clear();
        output
            .try_reserve_exact(SAVE_LEN)
            .map_err(|_| PersistenceError::Capacity)?;
        output.resize(SAVE_LEN, 0);
        model
            .encode_progress(output)
            .map_err(|_| PersistenceError::Encoding)?;
        Ok(EncodeOutcome::Encoded)
    }

    fn prepare(&self, _: &FactoryModel, bytes: &[u8]) -> Result<FactoryProgress, PersistenceError> {
        FactoryModel::decode_progress(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut FactoryModel, prepared: FactoryProgress) {
        model.apply_progress(prepared);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_module_factory.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("module_factory persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(FactoryArchive)
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
    use crate::gallery::play::factory::FactoryModal;
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    fn finished_order(mission: u8) -> FactoryModel {
        let mut model = FactoryModel::default();
        model.request_mission(mission, true);
        while model.step_once().is_ok() && model.modal() != FactoryModal::Result {}
        model.close_modal();
        model
    }

    #[test]
    fn codec_restores_stars_and_the_open_order_and_reuses_its_buffer() {
        let source = finished_order(2);
        let mut bytes = Vec::new();
        FactoryArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(&bytes[..4], b"MFC1");
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    FactoryArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("module_factory/progress", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(FactoryArchive));
        let model = app.add_model(FactoryModel::default()).finish();
        assert_eq!(model.mission_index(), 2);
        assert_eq!(model.stars()[2], 3);
        assert_eq!(model.completed(), 1 << 2);
        assert_eq!(model.modal(), FactoryModal::None);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);

        model.request_mission(0, false);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.mission_index(), 0);
        assert_eq!(model.stars()[2], 3);
    }

    #[test]
    fn invalid_archive_keeps_a_fresh_start_and_blocks_automatic_overwrite() {
        let mut storage = MemoryStorage::new();
        storage.write("module_factory/progress", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(FactoryArchive));
        let model = app.add_model(FactoryModel::default()).finish();
        assert_eq!(model.mission_index(), 0);
        assert_eq!(model.completed(), 0);
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
