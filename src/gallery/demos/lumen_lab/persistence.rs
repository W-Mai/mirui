use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::lumen::{LumenModel, LumenProgress, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// Keeps the best stars of every puzzle and the one in play.
pub(super) struct LumenArchive;

impl ModelCodec for LumenArchive {
    type Model = LumenModel;
    type Prepared = LumenProgress;

    fn encode(
        &self,
        model: &LumenModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
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

    fn prepare(&self, _: &LumenModel, bytes: &[u8]) -> Result<LumenProgress, PersistenceError> {
        LumenModel::decode_progress(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut LumenModel, progress: LumenProgress) {
        model.restore(progress);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_lumen_lab.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("lumen_lab progress is not saved: {error}");
            return;
        }
    };
    app.add_plugin(
        PersistencePlugin::new(storage)
            .codec(LumenArchive)
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

    fn solved_first_then_on_the_fourth() -> LumenModel {
        let mut model = LumenModel::new();
        model.rotate(1);
        model.select_level(3);
        model
    }

    #[test]
    fn stars_and_the_puzzle_in_play_survive_a_restart() {
        let source = solved_first_then_on_the_fourth();
        let mut bytes = Vec::new();
        LumenArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    LumenArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("lumen_lab/progress", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(LumenArchive));
        let model = app.add_model(LumenModel::new()).finish();
        assert_eq!(model.level_index(), 3);
        assert_eq!(model.stars()[0], 3);
        assert_eq!(model.total_stars(), 3);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.select_level(0);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.level_index(), 0);
    }

    #[test]
    fn a_broken_archive_keeps_the_fresh_game_and_is_not_overwritten() {
        let mut storage = MemoryStorage::new();
        storage.write("lumen_lab/progress", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(LumenArchive));
        let model = app.add_model(LumenModel::new()).finish();
        assert_eq!(model.level_index(), 0);
        assert_eq!(model.total_stars(), 0);
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
