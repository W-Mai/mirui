use crate::core::persistence::{EncodeOutcome, ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::picture::{PictureModel, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

struct PictureArchive;

impl ModelCodec for PictureArchive {
    type Model = PictureModel;
    type Prepared = PictureModel;

    fn encode(
        &self,
        model: &PictureModel,
        output: &mut alloc::vec::Vec<u8>,
    ) -> Result<EncodeOutcome, PersistenceError> {
        if model.stroke_active() {
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

    fn prepare(&self, _: &PictureModel, bytes: &[u8]) -> Result<PictureModel, PersistenceError> {
        PictureModel::decode(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut PictureModel, prepared: PictureModel) {
        *model = prepared;
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_atlas_restoration.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("atlas_restoration persistence unavailable: {error}");
            return;
        }
    };

    let plugin = PersistencePlugin::new(storage)
        .codec(PictureArchive)
        .autosave(1000);
    app.add_plugin(plugin);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        persistence::PersistenceStatus,
        storage::{MemoryStorage, Storage, StorageError, WriteOutcome},
    };
    use crate::test_support::tracked_allocations;
    use alloc::{rc::Rc, vec::Vec};
    use core::cell::{Cell, RefCell};

    const KEY: &str = "atlas_restoration/save";

    #[derive(Clone, Default)]
    struct Store {
        data: Rc<RefCell<MemoryStorage>>,
        writes: Rc<Cell<usize>>,
    }

    impl Storage for Store {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            self.data.borrow().read(key)
        }

        fn write(&mut self, key: &str, bytes: &[u8]) -> Result<WriteOutcome, StorageError> {
            self.writes.set(self.writes.get() + 1);
            self.data.borrow_mut().write(key, bytes)
        }

        fn remove(&mut self, key: &str) -> Result<(), StorageError> {
            self.data.borrow_mut().remove(key)
        }
    }

    fn editable_cell(model: &PictureModel) -> u8 {
        let level = model.level();
        (0..level.size() * level.size())
            .find(|cell| !level.is_given(*cell))
            .expect("Picture level has an editable cell")
    }

    #[test]
    fn codec_restores_registered_state_and_reuses_encoding_storage() {
        let mut source = PictureModel::default();
        let cell = editable_cell(&source);
        source.begin_stroke(cell);
        source.end_stroke(false);
        let mut bytes = Vec::new();
        assert_eq!(
            PictureArchive.encode(&source, &mut bytes).unwrap(),
            EncodeOutcome::Encoded
        );
        let mut expected = [0; SAVE_LEN];
        source.encode_into(&mut expected).unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    PictureArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write(KEY, &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PictureArchive));
        let model = app.add_model(PictureModel::default()).finish();
        assert_eq!(model.history_len(), 1);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.restart();
        assert_eq!(model.history_len(), 0);
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.history_len(), 1);
        model.restart();
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.history_len(), 0);
    }

    #[test]
    fn active_strokes_keep_the_archive_until_completion_or_cancellation() {
        let store = Store::default();
        let source = PictureModel::default();
        let cell = editable_cell(&source);
        let mut initial = Vec::new();
        PictureArchive.encode(&source, &mut initial).unwrap();
        store.data.borrow_mut().write(KEY, &initial).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(store.clone()).codec(PictureArchive));
        let model = app.add_model(source).finish();

        for cancelled in [false, true] {
            let previous = store.read(KEY).unwrap();
            let writes = store.writes.get();
            model.begin_stroke(cell);
            assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
            app.persistence().save_all().unwrap();
            app.persistence().save(&model).unwrap();
            app.suspend();
            assert_eq!(store.writes.get(), writes);
            assert_eq!(store.read(KEY).unwrap(), previous);
            assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
            app.resume();
            model.end_stroke(cancelled);
            app.persistence().save_all().unwrap();
            assert_eq!(store.writes.get(), writes + 1);
            assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
            let saved = store.read(KEY).unwrap().unwrap();
            let restored = PictureModel::decode(&saved).unwrap();
            assert!(!restored.stroke_active());
            assert_eq!(restored.history_len(), 1);
            if cancelled {
                assert_eq!(Some(saved), previous);
            } else {
                assert_ne!(Some(saved), previous);
                assert_ne!(restored.cell(cell), source.cell(cell));
            }
        }
    }

    #[test]
    fn invalid_archive_preserves_initial_data_and_blocks_automatic_overwrite() {
        let mut storage = MemoryStorage::new();
        storage.write(KEY, b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PictureArchive));
        let model = app.add_model(PictureModel::default()).finish();
        assert_eq!(model.history_len(), 0);
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
