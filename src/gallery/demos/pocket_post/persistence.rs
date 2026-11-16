use crate::core::persistence::EncodeOutcome;
use crate::core::persistence::{ModelCodec, PersistenceError, PersistencePlugin};
use crate::gallery::play::post::{PostModel, PostProgress, SAVE_LEN};
use crate::prelude::{App, RendererFactory, Surface};

/// Keeps the stars and best score of every shift, and the shift in play.
pub(super) struct PostArchive;

impl ModelCodec for PostArchive {
    type Model = PostModel;
    type Prepared = PostProgress;

    fn encode(
        &self,
        model: &PostModel,
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

    fn prepare(&self, _: &PostModel, bytes: &[u8]) -> Result<PostProgress, PersistenceError> {
        PostModel::decode_progress(bytes).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn apply(&self, model: &mut PostModel, progress: PostProgress) {
        model.restore(progress);
    }
}

pub(super) fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::gallery::play::storage::gallery_storage;

    let storage = match gallery_storage("mirui_pocket_post.bin") {
        Ok(storage) => storage,
        Err(error) => {
            crate::warn!("pocket_post progress is not saved: {error}");
            return;
        }
    };
    app.add_plugin(
        PersistencePlugin::new(storage)
            .codec(PostArchive)
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
    use crate::gallery::play::post::PostRoute;
    use crate::test_support::tracked_allocations;
    use alloc::vec::Vec;

    /// Sets each switch for the parcel that reaches it next.
    fn steer(model: &mut PostModel) {
        let leader = |route: PostRoute| {
            (0..usize::from(model.active_len()))
                .filter_map(|index| model.active(index))
                .filter(|parcel| parcel.route() == route)
                .max_by_key(|parcel| parcel.distance())
        };
        let first = leader(PostRoute::Entry).map(|parcel| u8::from(parcel.target() != 0));
        let second = leader(PostRoute::ToSecondSwitch).map(|parcel| u8::from(parcel.target() == 2));
        if first.is_some_and(|branch| model.switch(0) != branch) {
            model.toggle_switch(0);
        }
        if second.is_some_and(|branch| model.switch(1) != branch) {
            model.toggle_switch(1);
        }
    }

    /// Finishes the first shift without a slip and moves to the third.
    fn starred_first_then_on_the_third() -> PostModel {
        let mut model = PostModel::default();
        model.toggle_running();
        for _ in 0..40_000 {
            steer(&mut model);
            model.advance_ms(17);
            if model.finished() {
                break;
            }
        }
        assert_eq!(model.starred(), 1);
        model.load_shift(2);
        model
    }

    #[test]
    fn stars_best_scores_and_the_shift_survive_a_restart() {
        let source = starred_first_then_on_the_third();
        let mut bytes = Vec::new();
        PostArchive.encode(&source, &mut bytes).unwrap();
        assert_eq!(bytes.len(), SAVE_LEN);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    PostArchive.encode(&source, &mut bytes).unwrap(),
                    EncodeOutcome::Encoded
                );
            }),
            0
        );
        let mut storage = MemoryStorage::new();
        storage.write("pocket_post/progress", &bytes).unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PostArchive));
        let model = app.add_model(PostModel::default()).finish();
        assert_eq!(model.shift(), 2);
        assert_eq!(model.best(), source.best());
        assert_eq!(model.starred(), source.starred());
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
        model.load_shift(0);
        assert_eq!(app.persistence().status(&model), PersistenceStatus::Pending);
        app.persistence().save_all().unwrap();
        app.persistence().restore(&model).unwrap();
        assert_eq!(model.shift(), 0);
    }

    #[test]
    fn a_broken_archive_keeps_the_fresh_day_and_is_not_overwritten() {
        let mut storage = MemoryStorage::new();
        storage.write("pocket_post/progress", b"broken").unwrap();
        let mut app = App::headless(16, 16);
        app.add_plugin(PersistencePlugin::new(storage).codec(PostArchive));
        let model = app.add_model(PostModel::default()).finish();
        assert_eq!(model.shift(), 0);
        assert_eq!(model.starred(), 0);
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
