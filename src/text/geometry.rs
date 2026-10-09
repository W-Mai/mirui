//! Borrowed geometry backed by the retained text-layout and path-placement arenas.

use core::cell::Ref;

use crate::ecs::{Entity, World};
use crate::render::font::FontMetrics;
use crate::types::{Rect, Transform3D};
use crate::ui::outputs::{Output, OutputStoreError, Outputs, private::OutputKind};

use super::TextPath;
use super::baseline::{PathTextRuntime, PlacementKey};
use super::layout::{TextLayout, TextLayoutCache, TextLayoutHandle, TextLayoutLimits};

/// Prepared text geometry published for an entity with a `Text` component.
pub struct TextGeometry;

#[doc(hidden)]
pub struct TextGeometryStorage {
    pub(crate) layouts: TextLayoutCache,
    pub(crate) paths: PathTextRuntime,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextGeometryRecord {
    pub(crate) layout: TextLayoutHandle,
    pub(crate) placement: Option<PlacementKey>,
    pub(crate) metrics: FontMetrics,
    pub(crate) content_rect: Rect,
    pub(crate) transform: Transform3D,
    pub(crate) text_path: Option<TextPath>,
    pub(crate) caret_ready: bool,
}

/// A shared borrow of one successfully prepared text result.
pub struct TextGeometryRead<'a> {
    storage: Ref<'a, TextGeometryStorage>,
    record: TextGeometryRecord,
}

impl TextGeometryRecord {
    fn is_readable(self, storage: &TextGeometryStorage) -> bool {
        storage.layouts.get(self.layout).is_some()
            && self
                .placement
                .is_none_or(|key| storage.paths.glyph_frames(key, |_| ()).is_some())
    }
}

impl Output for TextGeometry {
    type Storage = TextGeometryStorage;
    type Record = TextGeometryRecord;
    type Read<'a> = TextGeometryRead<'a>;

    fn read<'a>(storage: Ref<'a, Self::Storage>, record: Self::Record) -> Option<Self::Read<'a>> {
        record
            .is_readable(&storage)
            .then_some(TextGeometryRead { storage, record })
    }
}

impl OutputKind for TextGeometry {
    fn install(world: &mut World) {
        if world.resource::<Outputs<Self>>().is_none() {
            world.insert_resource(Outputs::<Self>::new_text(TextLayoutLimits::default()));
        }
    }

    fn reconcile(world: &World) {
        let Some(outputs) = world.resource::<Outputs<Self>>() else {
            return;
        };
        outputs.reset_where(|entity| {
            !world.is_alive(entity)
                || !world.has::<crate::ui::widgets::Text>(entity)
                || crate::ui::branch::is_hidden_in_tree(world, entity)
        });
        outputs.retain_entities(|_| false);
    }
}

impl Outputs<TextGeometry> {
    pub(crate) fn new_text(limits: TextLayoutLimits) -> Self {
        Self::new(TextGeometryStorage {
            layouts: TextLayoutCache::new(limits),
            paths: PathTextRuntime::default(),
        })
    }

    pub(crate) fn replace_text_layouts(&self, cache: TextLayoutCache) {
        {
            let mut storage = self.storage_mut();
            storage.layouts = cache;
            storage.paths.trim_memory();
        }
        self.reset_all();
    }

    pub(crate) fn trim_text_memory(&self) {
        {
            let mut storage = self.storage_mut();
            storage.layouts.trim_memory();
            storage.paths.trim_memory();
        }
        self.reset_all();
    }

    pub(crate) fn publish_text(
        &self,
        entity: Entity,
        record: TextGeometryRecord,
    ) -> Result<(), OutputStoreError> {
        if !self.is_requested(entity) {
            return Ok(());
        }
        if !record.is_readable(&self.storage()) {
            self.invalidate(entity);
            return Err(OutputStoreError::Missing(entity));
        }
        let state = self
            .state(entity)
            .ok_or(OutputStoreError::Missing(entity))?;
        if state
            .current
            .is_some_and(|current| current.record == record)
            && self.revalidate_current(entity, state.input_version)
        {
            return Ok(());
        }
        let input_version = state
            .input_version
            .checked_add(1)
            .ok_or(OutputStoreError::PublicationExhausted)?;
        self.prepare(entity, input_version)?;
        self.publish(entity, input_version, record)?;
        Ok(())
    }
}

impl TextGeometryRead<'_> {
    /// Borrow the retained layout without producing or copying glyph data.
    pub fn layout(&self) -> TextLayout<'_> {
        self.storage
            .layouts
            .get(self.record.layout)
            .expect("a text geometry borrow keeps its validated layout alive")
    }

    /// Font metrics captured with the prepared layout.
    pub const fn metrics(&self) -> FontMetrics {
        self.record.metrics
    }

    /// The text content rectangle in source-local coordinates.
    pub const fn content_rect(&self) -> Rect {
        self.record.content_rect
    }

    /// The prepared source-local to screen transform, including scroll offsets.
    pub const fn transform(&self) -> Transform3D {
        self.record.transform
    }

    /// The prepared baseline path, if this result contains path text.
    pub const fn text_path(&self) -> Option<TextPath> {
        self.record.text_path
    }

    pub(crate) fn with_path_carets<R>(
        &self,
        inspect: impl FnOnce(
            &textflow::layout::ParagraphLayout<'_>,
            &[textflow::placement::CaretFrame],
            FontMetrics,
        ) -> R,
    ) -> Option<R> {
        if !self.record.caret_ready {
            return None;
        }
        let key = self.record.placement?;
        let layout = self.layout();
        let paragraph = layout.paragraph().ok()?;
        self.storage.paths.caret_frames(key, |frames| {
            inspect(&paragraph, frames, self.record.metrics)
        })
    }
}

#[cfg(test)]
#[path = "geometry_tests.rs"]
mod tests;
