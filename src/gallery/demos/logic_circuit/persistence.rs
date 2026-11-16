use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::circuit::{CircuitModel, CircuitProgress, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// Keeps the stars of every task and the open task.
pub(super) struct CircuitArchive;

impl ModelCodec for CircuitArchive {
    type Model = CircuitModel;
    type Prepared = CircuitProgress;

    fn encode(
        &self,
        model: &CircuitModel,
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

    fn prepare(&self, _: &CircuitModel, bytes: &[u8]) -> Result<CircuitProgress, PersistenceError> {
        CircuitModel::decode_progress(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut CircuitModel, prepared: CircuitProgress) {
        model.apply_progress(prepared);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_logic_circuit.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("logic_circuit persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(CircuitArchive)
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
    use crate::gallery::play::circuit::CircuitModal;
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    #[test]
    fn codec_restores_stars_and_the_open_task_and_reuses_its_buffer() {
        let mut source = CircuitModel::default();
        source.load_task(4, true);
        source.verify();
        source.close_modal();
        let mut bytes = Vec::new();
        CircuitArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(&bytes[..4], b"LGC1");
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    CircuitArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("logic_circuit/progress", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(CircuitArchive));
        let model = app.add_model(CircuitModel::default()).finish();
        assert_eq!(model.task(), 4);
        assert_eq!(model.stars()[4], 3);
        assert_eq!(model.modal(), CircuitModal::None);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);

        model.choose_task(0);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.task(), 0);
        assert_eq!(model.stars()[4], 3);
    }

    #[test]
    fn invalid_archive_keeps_a_fresh_start_and_blocks_automatic_overwrite() {
        let mut storage = MemoryStorage::new();
        storage.write("logic_circuit/progress", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(CircuitArchive));
        let model = app.add_model(CircuitModel::default()).finish();
        assert_eq!(model.task(), 0);
        assert_eq!(model.solved_mask(), 0);
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
