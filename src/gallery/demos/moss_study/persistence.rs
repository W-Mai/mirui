use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::moss::{MossGarden, MossModel, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// Keeps the garden as it stands, its generation, seed and pace.
pub(super) struct MossArchive;

impl ModelCodec for MossArchive {
    type Model = MossModel;
    type Prepared = MossGarden;

    fn encode(
        &self,
        model: &MossModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        // A drag in progress is saved once it is committed or cancelled.
        if model.stroking() {
            return Ok(EncodeOutcome::Deferred);
        }
        output.clear();
        output
            .try_reserve_exact(SAVE_LEN)
            .map_err(|_| PersistenceError::Capacity)?;
        output.resize(SAVE_LEN, 0);
        model
            .encode_into(output)
            .map_err(|_| PersistenceError::Encoding)?;
        Ok(EncodeOutcome::Encoded)
    }

    fn prepare(&self, _: &MossModel, bytes: &[u8]) -> Result<MossGarden, PersistenceError> {
        MossModel::decode_garden(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut MossModel, garden: MossGarden) {
        model.restore_garden(garden);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_moss_study.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("moss_study garden is not saved: {error}");
            return;
        }
    };
    app.add_plugin(
        PersistencePlugin::new(storage)
            .codec(MossArchive)
            .autosave(1000),
    );
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
    fn the_garden_survives_a_restart_and_a_drag_waits() {
        let mut source = MossModel::default();
        source.step();
        source.paint_cell(0, 0);
        let mut bytes = Vec::new();
        MossArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    MossArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("moss_study/garden", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(MossArchive));
        let model = app.add_model(MossModel::default()).finish();
        assert_eq!(model.generation(), 1);
        assert_eq!(model.live_count(), source.live_count());
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);

        let mut drawing = source;
        drawing.begin_stroke(5, 5);
        assert_eq!(
            MossArchive.encode(&drawing, &mut bytes).unwrap(),
            EncodeOutcome::Deferred
        );
    }

    #[test]
    fn a_broken_archive_keeps_the_seed_garden_and_is_not_overwritten() {
        let mut storage = MemoryStorage::new();
        storage.write("moss_study/garden", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(MossArchive));
        let model = app.add_model(MossModel::default()).finish();
        assert_eq!(model.generation(), 0);
        assert_eq!(model.live_count(), 15);
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
