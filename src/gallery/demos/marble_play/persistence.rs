use crate::core::persistence::{EncodeOutcome, ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::marble::{MarbleLayout, MarbleModel, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// The scene, its settings and the pads, under `marble_play/layout`.
pub(super) struct MarbleArchive;

impl ModelCodec for MarbleArchive {
    type Model = MarbleModel;
    type Prepared = MarbleLayout;

    fn encode(
        &self,
        model: &MarbleModel,
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

    fn prepare(&self, _: &MarbleModel, bytes: &[u8]) -> Result<MarbleLayout, PersistenceError> {
        MarbleLayout::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut MarbleModel, prepared: MarbleLayout) {
        model.restore(prepared);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_marble_play.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("marble_play persistence unavailable: {error}");
            return;
        }
    };
    app.add_plugin(
        PersistencePlugin::new(storage)
            .codec(MarbleArchive)
            .autosave(1000),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::persistence::PersistenceStatus;
    use crate::core::storage::{MemoryStorage, Storage};
    use crate::gallery::play::marble::Page;
    use crate::test_support::tracked_allocations;
    use crate::types::Fixed64;
    use alloc::vec::Vec;

    #[test]
    fn the_pads_come_back_after_a_restart() {
        let mut source = MarbleModel::new();
        let _ = source.reset_scene(1);
        let _ = source.set_page(Page::Edit);
        let _ = source.remove_selected();
        let _ = source.set_bpm(Fixed64::from_int(120));
        let mut bytes = Vec::new();
        MarbleArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(&bytes[..4], b"MRB1");
        assert_eq!(
            tracked_allocations(|| {
                MarbleArchive.encode(&source, &mut bytes).unwrap();
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("marble_play/layout", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(MarbleArchive));
        let model = app.add_model(MarbleModel::new()).finish();
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        assert_eq!(model.scene(), 1);
        assert_eq!(model.bpm(), 120);
        assert_eq!(model.pad_count(), 4);
        assert_eq!(model.page(), Page::Play);
    }

    #[test]
    fn a_broken_save_keeps_the_first_scene_and_is_not_overwritten() {
        let mut storage = MemoryStorage::new();
        storage.write("marble_play/layout", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(MarbleArchive));
        let model = app.add_model(MarbleModel::new()).finish();
        assert_eq!(model.scene(), 0);
        assert_eq!(model.pad_count(), 5);
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
