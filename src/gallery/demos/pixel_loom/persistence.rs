use crate::core::persistence::{EncodeOutcome, ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::pixel::{PixelModel, PixelSave, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// The four frames and the editor settings, under `pixel_loom/frames`.
pub(super) struct PixelArchive;

impl ModelCodec for PixelArchive {
    type Model = PixelModel;
    type Prepared = PixelSave;

    fn encode(
        &self,
        model: &PixelModel,
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

    fn prepare(&self, _: &PixelModel, bytes: &[u8]) -> Result<PixelSave, PersistenceError> {
        PixelSave::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut PixelModel, prepared: PixelSave) {
        model.restore(prepared);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_pixel_loom.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("pixel_loom persistence unavailable: {error}");
            return;
        }
    };
    app.add_plugin(
        PersistencePlugin::new(storage)
            .codec(PixelArchive)
            .autosave(1000),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::persistence::PersistenceStatus;
    use crate::core::storage::{MemoryStorage, Storage};
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    #[test]
    fn a_drawing_comes_back_after_a_restart() {
        let mut source = PixelModel::default();
        source.select_frame(1);
        source.paint_cell(3, 4);
        source.cycle_fps();
        let mut bytes = Vec::new();
        PixelArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(&bytes[..4], b"PXL1");
        assert_eq!(
            tracked_allocations(|| {
                PixelArchive.encode(&source, &mut bytes).unwrap();
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("pixel_loom/frames", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PixelArchive));
        let model = app.add_model(PixelModel::default()).finish();
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        assert_eq!(model.frame(), 1);
        assert_eq!(model.fps(), source.fps());
        crate::core::model::ModelHandle::read(&model, |model| {
            assert_eq!(model.frames(), source.frames());
            assert!(!model.can_undo());
        });
    }

    #[test]
    fn a_broken_save_keeps_the_template_and_is_not_overwritten() {
        let mut storage = MemoryStorage::new();
        storage.write("pixel_loom/frames", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PixelArchive));
        let model = app.add_model(PixelModel::default()).finish();
        assert_eq!(model.template_id(), 0);
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
