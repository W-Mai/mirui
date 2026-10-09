use super::BufferKey;
use crate::ecs::Entity;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Rect};
use crate::ui::outputs::{
    HistoryOutput, Output, Outputs,
    private::{HistoryKind, OutputKind},
    workspace::PaintWorkspace,
};
use core::cell::Ref;
use hashbrown::HashMap;

/// The completed pixel output of a widget subtree.
pub struct WidgetTexture;

/// A shared borrow of completed widget pixels and their published logical geometry.
pub struct WidgetTextureRead<'a> {
    texture: Ref<'a, Texture<'static>>,
    layout_rect: Rect,
}

impl WidgetTextureRead<'_> {
    pub fn texture(&self) -> &Texture<'static> {
        &self.texture
    }

    /// Source rectangle in root layout coordinates, excluding scrolling and external transforms.
    pub fn layout_rect(&self) -> Rect {
        self.layout_rect
    }

    /// Logical bounds relative to the source layout origin, without visual overflow.
    pub fn local_bounds(&self) -> Rect {
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: self.layout_rect.w,
            h: self.layout_rect.h,
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct WidgetTextureRecord {
    entity: Entity,
    slot: usize,
    key: BufferKey,
}

struct TextureSlots {
    planes: [Option<Texture<'static>>; 2],
    shape: (u16, u16, ColorFormat),
    writing: bool,
    discard: bool,
}

#[doc(hidden)]
pub struct WidgetTextureStorage {
    textures: HashMap<Entity, TextureSlots>,
    budget: usize,
    allocated: usize,
    pub(crate) plan: Option<crate::ui::render_system::output_plan::TexturePlan>,
    deferred_workspace_trim: bool,
}

impl OutputKind for WidgetTexture {
    fn install(world: &mut crate::ecs::World) {
        if world.resource::<Outputs<Self>>().is_none() {
            world.insert_resource(Outputs::<Self>::default());
        }
    }

    fn reconcile(world: &crate::ecs::World) {
        super::reconcile_texture_outputs(world);
    }
}
impl HistoryKind for WidgetTexture {}

impl Output for WidgetTexture {
    type Storage = WidgetTextureStorage;
    type Record = WidgetTextureRecord;
    type Read<'a> = WidgetTextureRead<'a>;

    fn read<'a>(storage: Ref<'a, Self::Storage>, record: Self::Record) -> Option<Self::Read<'a>> {
        Ref::filter_map(storage, |storage| {
            storage.textures.get(&record.entity)?.planes[record.slot].as_ref()
        })
        .ok()
        .map(|texture| WidgetTextureRead {
            texture,
            layout_rect: record.key.layout_rect,
        })
    }
}

impl HistoryOutput for WidgetTexture {}

impl Outputs<WidgetTexture> {
    /// Reserve a byte budget for widget output pixels. Zero disables offscreen storage.
    pub fn with_budget(budget: usize) -> Self {
        Self::new(WidgetTextureStorage {
            textures: HashMap::new(),
            budget,
            allocated: 0,
            plan: Some(Default::default()),
            deferred_workspace_trim: false,
        })
    }

    pub(crate) fn take_texture_plan(
        &self,
    ) -> Option<crate::ui::render_system::output_plan::TexturePlan> {
        self.storage_mut().plan.take()
    }

    pub(crate) fn put_texture_plan(
        &self,
        mut plan: crate::ui::render_system::output_plan::TexturePlan,
    ) {
        let mut storage = self.storage_mut();
        assert!(storage.plan.is_none(), "texture plan is already stored");
        if storage.deferred_workspace_trim {
            storage.allocated -= plan.workspace.release();
            storage.deferred_workspace_trim = false;
        }
        storage.plan = Some(plan);
    }

    pub(crate) fn reserve_workspace(
        &self,
        workspace: &mut PaintWorkspace,
        required_bytes: usize,
    ) -> bool {
        let mut storage = self.storage_mut();
        if storage.plan.is_some() {
            return false;
        }
        let previous_capacity = workspace.capacity();
        let Some(without_workspace) = storage.allocated.checked_sub(previous_capacity) else {
            return false;
        };
        if required_bytes == 0 {
            workspace.release();
            storage.allocated = without_workspace;
            return true;
        }
        if storage.deferred_workspace_trim {
            return false;
        }
        if required_bytes <= previous_capacity {
            return true;
        }
        let available = storage.budget.saturating_sub(without_workspace);
        if required_bytes > available {
            return false;
        }
        // No old allocation remains live while the larger buffer is allocated.
        workspace.release();
        storage.allocated = without_workspace;
        if !workspace.allocate(required_bytes) {
            workspace.release();
            return false;
        }
        let actual_capacity = workspace.capacity();
        if actual_capacity > available {
            workspace.release();
            return false;
        }
        storage.allocated += actual_capacity;
        true
    }

    pub(crate) fn texture_matches(&self, key: BufferKey) -> bool {
        self.state(key.entity).is_some_and(|state| {
            state.current_available
                && state.current.is_some_and(|current| {
                    current.record.key == key && current.input_version == key.generation as u64
                })
                && self.storage().textures.contains_key(&key.entity)
        })
    }

    pub(crate) fn reserve_texture(&self, key: BufferKey) -> bool {
        if self.texture_matches(key) {
            return true;
        }
        self.reserve_texture_slot(key).is_some()
    }

    fn reserve_texture_slot(&self, key: BufferKey) -> Option<()> {
        if key.w == 0 || key.h == 0 {
            self.reset_texture(key.entity);
            return None;
        }
        if self
            .state(key.entity)
            .is_some_and(|state| state.retained && state.published_in_batch)
        {
            return None;
        }
        let shape = (key.w, key.h, key.format);
        let bytes = (key.w as usize)
            .checked_mul(key.format.bytes_per_pixel())?
            .checked_mul(key.h as usize)?;
        if self.state(key.entity).is_none() {
            self.reserve(1).ok()?;
        }
        let mut storage = self.storage_mut();
        if storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.writing)
        {
            return None;
        }
        let logical_size_changed = self.state(key.entity).is_some_and(|state| {
            state.current.or(state.previous).is_some_and(|published| {
                let previous = published.record.key.layout_rect;
                previous.w != key.layout_rect.w || previous.h != key.layout_rect.h
            })
        });
        let pixel_shape_changed = storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.shape != shape);
        if pixel_shape_changed {
            let old = storage.textures.remove(&key.entity)?;
            storage.allocated -= old
                .planes
                .iter()
                .flatten()
                .map(|plane| plane.buf.as_slice().len())
                .sum::<usize>();
        }
        if pixel_shape_changed || logical_size_changed {
            self.reset(key.entity);
        }
        // Geometry changes invalidate current even when generation is unchanged.
        self.invalidate(key.entity);
        self.prepare(key.entity, key.generation as u64).ok()?;
        let state = self.state(key.entity)?;
        let slot = if state.retained {
            state
                .previous
                .map_or(0, |previous| 1 - previous.record.slot)
        } else {
            state.current.map_or(0, |current| current.record.slot)
        };
        let reserved = storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.planes[slot].is_some());
        if reserved {
            return Some(());
        }
        let total = storage.allocated.checked_add(bytes)?;
        if total > storage.budget {
            return None;
        }
        if !storage.textures.contains_key(&key.entity) {
            storage.textures.try_reserve(1).ok()?;
        }
        let slots = storage
            .textures
            .entry(key.entity)
            .or_insert_with(|| TextureSlots {
                planes: [None, None],
                shape,
                writing: false,
                discard: false,
            });
        slots.planes[slot] = Some(Texture::owned(key.w, key.h, key.format));
        storage.allocated = total;
        Some(())
    }

    pub(crate) fn prepare_texture(&self, key: BufferKey) -> Option<TextureWrite<'_>> {
        if key.w == 0 || key.h == 0 {
            self.reset_texture(key.entity);
            return None;
        }
        if self
            .state(key.entity)
            .is_some_and(|state| state.retained && state.published_in_batch)
        {
            return None;
        }
        // Every write request invalidates current, including a same-key rewrite.
        // Reservation itself leaves matching published pixels untouched.
        self.invalidate(key.entity);
        if !self.reserve_texture(key) {
            return None;
        }
        let state = self.state(key.entity)?;
        let slot = if state.retained {
            state
                .previous
                .map_or(0, |previous| 1 - previous.record.slot)
        } else {
            state.current.map_or(0, |current| current.record.slot)
        };
        let mut storage = self.storage_mut();
        let slots = storage.textures.get_mut(&key.entity)?;
        let texture = slots.planes[slot].take()?;
        slots.writing = true;
        drop(storage);
        let mut texture = texture;
        texture.buf.as_mut_slice().fill(0);
        Some(TextureWrite {
            outputs: self,
            record: WidgetTextureRecord {
                entity: key.entity,
                slot,
                key,
            },
            texture: Some(texture),
        })
    }

    fn reset_texture(&self, entity: Entity) {
        self.reset(entity);
        let mut storage = self.storage_mut();
        let Some(slots) = storage.textures.get_mut(&entity) else {
            return;
        };
        let mut released = 0;
        for plane in &mut slots.planes {
            if let Some(plane) = plane.take() {
                released += plane.buf.as_slice().len();
            }
        }
        slots.discard = true;
        if !slots.writing {
            storage.textures.remove(&entity);
        }
        storage.allocated -= released;
    }

    pub(crate) fn trim_memory(&self) {
        let mut storage = self.storage_mut();
        let mut released = 0;
        storage.textures.retain(|entity, slots| {
            self.reset(*entity);
            for plane in &mut slots.planes {
                if let Some(plane) = plane.take() {
                    released += plane.buf.as_slice().len();
                }
            }
            slots.discard = true;
            slots.writing
        });
        storage.allocated -= released;
        if let Some(plan) = storage.plan.as_mut() {
            let released = plan.workspace.release();
            storage.allocated -= released;
        } else {
            storage.deferred_workspace_trim = true;
        }
    }

    pub(crate) fn reconcile_textures(
        &self,
        requested: impl Fn(Entity) -> bool,
        visible: impl Fn(Entity) -> bool,
    ) {
        let mut storage = self.storage_mut();
        let mut released = 0;
        storage.textures.retain(|entity, slots| {
            let demanded = self.is_requested(*entity) || requested(*entity);
            if demanded && visible(*entity) {
                if let Some(state) = self.state(*entity)
                    && !state.retained
                    && !slots.writing
                {
                    let keep = state.current.map(|current| current.record.slot);
                    for (index, plane) in slots.planes.iter_mut().enumerate() {
                        if keep != Some(index)
                            && let Some(plane) = plane.take()
                        {
                            released += plane.buf.as_slice().len();
                        }
                    }
                }
                return true;
            }
            released += slots
                .planes
                .iter()
                .flatten()
                .map(|plane| plane.buf.as_slice().len())
                .sum::<usize>();
            slots.planes = [None, None];
            slots.discard = true;
            if demanded {
                self.reset(*entity);
            } else {
                self.release(*entity);
            }
            slots.writing
        });
        storage.allocated -= released;
        self.retain_entities(requested);
    }
}

impl Default for Outputs<WidgetTexture> {
    fn default() -> Self {
        Self::with_budget(0)
    }
}

pub(crate) struct TextureWrite<'a> {
    outputs: &'a Outputs<WidgetTexture>,
    record: WidgetTextureRecord,
    texture: Option<Texture<'static>>,
}

impl TextureWrite<'_> {
    pub(crate) fn texture_mut(&mut self) -> &mut Texture<'static> {
        self.texture.as_mut().expect("active output write")
    }

    pub(crate) fn publish(mut self) -> bool {
        self.finish(true)
    }

    fn finish(&mut self, publish: bool) -> bool {
        let Some(mut texture) = self.texture.take() else {
            return false;
        };
        let mut storage = self.outputs.storage_mut();
        if storage
            .textures
            .get(&self.record.entity)
            .is_some_and(|slots| slots.discard)
        {
            storage.allocated -= texture.buf.as_slice().len();
            storage.textures.remove(&self.record.entity);
            return false;
        }
        let published = publish
            && self
                .outputs
                .publish(
                    self.record.entity,
                    self.record.key.generation as u64,
                    self.record,
                )
                .is_ok();
        if published {
            texture.cache_revision = self
                .outputs
                .state(self.record.entity)
                .and_then(|state| state.current)
                .expect("published output")
                .version;
            texture.transient = false;
        }
        let slots = storage
            .textures
            .get_mut(&self.record.entity)
            .expect("reserved output slot");
        slots.planes[self.record.slot] = Some(texture);
        slots.writing = false;
        published
    }
}

impl Drop for TextureWrite<'_> {
    fn drop(&mut self) {
        self.finish(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::PlaneLayout;
    use crate::test_support::tracked_allocations;
    use crate::types::Color;

    const TEXTURE_BYTES: usize = 4 * 4 * 4;

    fn key(id: u32, generation: u32) -> BufferKey {
        BufferKey {
            entity: Entity { id, generation: 0 },
            layout_rect: Rect {
                w: Fixed::from_int(4),
                h: Fixed::from_int(4),
                ..Rect::ZERO
            },
            w: 4,
            h: 4,
            format: ColorFormat::RGBA8888,
            generation,
        }
    }

    #[test]
    fn geometry_belongs_to_successful_pixels_and_position_changes_preserve_history() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let entity = key(1, 1).entity;
        let original = BufferKey::for_widget(
            entity,
            Rect {
                x: Fixed::from_int(11) + Fixed::HALF,
                y: Fixed::from_int(-7),
                w: Fixed::from_int(8) + Fixed::HALF,
                h: Fixed::from_int(8) + Fixed::ONE / 4,
            },
            super::super::OffscreenRender::with_scale(Fixed::HALF),
            1,
        );
        let moved = BufferKey {
            layout_rect: Rect {
                x: original.layout_rect.x + Fixed::from_int(19),
                y: original.layout_rect.y - Fixed::HALF,
                ..original.layout_rect
            },
            ..original
        };
        let local_bounds = Rect {
            w: original.layout_rect.w,
            h: original.layout_rect.h,
            ..Rect::ZERO
        };
        let original_color = Color::rgb(17, 31, 47);
        let moved_color = Color::rgb(53, 71, 97);
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(original).unwrap();
            write.texture_mut().set_pixel(0, 0, &original_color);
            assert!(write.publish());
            let read = outputs.get(entity).unwrap();
            assert_eq!(read.layout_rect(), original.layout_rect);
            assert_eq!(read.local_bounds(), local_bounds);
            assert_eq!((read.texture().width, read.texture().height), (4, 4));
        }
        let _batch = outputs.begin_batch();
        assert!(outputs.texture_matches(original));
        assert!(!outputs.texture_matches(moved));
        assert!(outputs.reserve_texture(moved));
        assert!(outputs.get(entity).is_none());
        {
            let previous = outputs.previous(entity).unwrap();
            assert_eq!(previous.layout_rect(), original.layout_rect);
            assert_eq!(previous.local_bounds(), local_bounds);
            assert_eq!(previous.texture().get_pixel(0, 0), original_color);
        }

        let mut cancelled = outputs.prepare_texture(moved).unwrap();
        cancelled.texture_mut().set_pixel(0, 0, &moved_color);
        drop(cancelled);
        assert!(outputs.get(entity).is_none());
        assert_eq!(
            outputs.previous(entity).unwrap().layout_rect(),
            original.layout_rect
        );

        let mut write = outputs.prepare_texture(moved).unwrap();
        write.texture_mut().set_pixel(0, 0, &moved_color);
        assert!(write.publish());
        assert!(outputs.texture_matches(moved));
        assert!(!outputs.texture_matches(original));
        let read = outputs.get(entity).unwrap();
        assert_eq!(read.layout_rect(), moved.layout_rect);
        assert_eq!(read.local_bounds(), local_bounds);
        assert_eq!(read.texture().get_pixel(0, 0), moved_color);
        let previous = outputs.previous(entity).unwrap();
        assert_eq!(previous.layout_rect(), original.layout_rect);
        assert_eq!(previous.local_bounds(), local_bounds);
        assert_eq!(previous.texture().get_pixel(0, 0), original_color);
        assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
    }

    #[test]
    fn logical_size_changes_clear_history_without_reallocating_unchanged_pixel_shape() {
        for change_width in [true, false] {
            let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
            let entity = key(1, 1).entity;
            let original = BufferKey::for_widget(
                entity,
                Rect {
                    x: Fixed::from_int(3),
                    y: Fixed::from_int(5),
                    w: Fixed::from_int(4) + Fixed::ONE / 4,
                    h: Fixed::from_int(4) + Fixed::ONE / 4,
                },
                super::super::OffscreenRender::default(),
                2,
            );
            outputs.reserve(1).unwrap();
            outputs.set_retained(entity, true).unwrap();
            for generation in [1, 2] {
                let _batch = outputs.begin_batch();
                assert!(
                    outputs
                        .prepare_texture(BufferKey {
                            generation,
                            ..original
                        })
                        .unwrap()
                        .publish()
                );
            }
            let mut resized_rect = original.layout_rect;
            if change_width {
                resized_rect.w += Fixed::ONE / 4;
            } else {
                resized_rect.h += Fixed::ONE / 4;
            }
            let resized = BufferKey::for_widget(
                entity,
                resized_rect,
                super::super::OffscreenRender::default(),
                original.generation,
            );
            assert_eq!((resized.w, resized.h), (original.w, original.h));
            let _batch = outputs.begin_batch();
            assert!(outputs.previous(entity).is_some());
            assert!(!outputs.texture_matches(resized));

            let allocations = tracked_allocations(|| {
                assert!(outputs.reserve_texture(resized));
                assert!(outputs.get(entity).is_none());
                assert!(outputs.previous(entity).is_none());
                assert!(outputs.state(entity).unwrap().retained);
                drop(outputs.prepare_texture(resized).unwrap());
                assert!(outputs.get(entity).is_none());
                assert!(outputs.previous(entity).is_none());
                assert!(outputs.prepare_texture(resized).unwrap().publish());
                let read = outputs.get(entity).unwrap();
                assert_eq!(read.layout_rect(), resized_rect);
                assert_eq!(
                    read.local_bounds(),
                    Rect {
                        w: resized_rect.w,
                        h: resized_rect.h,
                        ..Rect::ZERO
                    }
                );
                assert!(outputs.previous(entity).is_none());
            });
            assert_eq!(allocations, 0);
            assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
        }
    }

    #[test]
    fn workspace_growth_shares_pixel_budget_and_preserves_other_outputs_on_rejection() {
        let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
        let source = key(1, 1);
        assert!(outputs.prepare_texture(source).unwrap().publish());
        let mut plan = outputs.take_texture_plan().unwrap();
        assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
        assert_eq!(
            outputs.storage().allocated,
            TEXTURE_BYTES + plan.workspace.capacity()
        );

        assert!(outputs.reserve_workspace(&mut plan.workspace, 2 * TEXTURE_BYTES));
        assert_eq!(plan.workspace.capacity(), 2 * TEXTURE_BYTES);
        assert_eq!(outputs.storage().allocated, 3 * TEXTURE_BYTES);
        assert!(!outputs.reserve_workspace(&mut plan.workspace, 3 * TEXTURE_BYTES));
        assert_eq!(plan.workspace.capacity(), 2 * TEXTURE_BYTES);
        assert_eq!(outputs.storage().allocated, 3 * TEXTURE_BYTES);
        assert!(outputs.get(source.entity).is_some());
        assert!(!outputs.reserve_texture(key(2, 1)));

        assert!(outputs.reserve_workspace(&mut plan.workspace, 0));
        assert_eq!(plan.workspace.capacity(), 0);
        assert_eq!(outputs.storage().allocated, TEXTURE_BYTES);
        assert!(
            outputs
                .prepare_texture(BufferKey { w: 8, ..key(2, 1) })
                .unwrap()
                .publish()
        );
        outputs.put_texture_plan(plan);
        assert_eq!(outputs.storage().allocated, 3 * TEXTURE_BYTES);
    }

    #[test]
    fn warm_workspace_leases_can_read_outputs_and_paint_without_allocating() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let source = key(1, 1);
        let expected = Color::rgba(17, 31, 47, 63);
        let mut write = outputs.prepare_texture(source).unwrap();
        write.texture_mut().set_pixel(0, 0, &expected);
        assert!(write.publish());
        let layout = PlaneLayout::packed(source.w, source.h, source.format).unwrap();
        let mut plan = outputs.take_texture_plan().unwrap();
        assert!(outputs.reserve_workspace(&mut plan.workspace, layout.required_bytes()));
        outputs.put_texture_plan(plan);

        let allocations = tracked_allocations(|| {
            for _ in 0..16 {
                let mut plan = outputs.take_texture_plan().unwrap();
                assert!(outputs.reserve_workspace(&mut plan.workspace, layout.required_bytes()));
                {
                    let source_read = outputs.get(source.entity).unwrap();
                    assert!(plan.workspace.with_texture(layout, |texture| {
                        texture.set_pixel(0, 0, &source_read.texture().get_pixel(0, 0));
                        assert_eq!(texture.get_pixel(0, 0), expected);
                    }));
                }
                outputs.put_texture_plan(plan);
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
    }

    #[test]
    fn trimming_an_active_plan_keeps_workspace_and_write_lease_budget_until_return() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let input = key(1, 1);
        let pending = outputs.prepare_texture(input).unwrap();
        let mut plan = outputs.take_texture_plan().unwrap();
        assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
        let layout = PlaneLayout::packed(input.w, input.h, input.format).unwrap();
        assert!(plan.workspace.with_texture(layout, |texture| {
            outputs.trim_memory();
            assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
            assert_eq!(plan.workspace.capacity(), TEXTURE_BYTES);
            assert!(!outputs.reserve_texture(key(2, 1)));
            texture.set_pixel(0, 0, &Color::rgb(11, 29, 47));
        }));
        assert!(!outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
        drop(pending);
        assert_eq!(outputs.storage().allocated, TEXTURE_BYTES);
        outputs.put_texture_plan(plan);
        assert_eq!(outputs.storage().allocated, 0);

        let plan = outputs.take_texture_plan().unwrap();
        assert_eq!(plan.workspace.capacity(), 0);
        outputs.put_texture_plan(plan);
        assert!(outputs.prepare_texture(key(2, 1)).unwrap().publish());
    }

    #[test]
    fn trimming_a_stored_plan_releases_its_workspace_immediately() {
        let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
        let mut plan = outputs.take_texture_plan().unwrap();
        assert!(outputs.reserve_workspace(&mut plan.workspace, TEXTURE_BYTES));
        outputs.put_texture_plan(plan);
        assert_eq!(outputs.storage().allocated, TEXTURE_BYTES);
        outputs.trim_memory();
        assert_eq!(outputs.storage().allocated, 0);
        let plan = outputs.take_texture_plan().unwrap();
        assert_eq!(plan.workspace.capacity(), 0);
        outputs.put_texture_plan(plan);
        assert!(outputs.prepare_texture(key(1, 1)).unwrap().publish());
    }

    #[test]
    fn reservation_preserves_pixels_until_the_write_is_detached() {
        let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
        let first = key(1, 1);
        let expected = Color::rgb(17, 39, 73);
        let mut write = outputs.prepare_texture(first).unwrap();
        write.texture_mut().set_pixel(0, 0, &expected);
        assert!(write.publish());

        let allocations = tracked_allocations(|| assert!(outputs.reserve_texture(first)));
        assert_eq!(allocations, 0);
        assert!(outputs.texture_matches(first));
        assert_eq!(
            outputs.get(first.entity).unwrap().texture().get_pixel(0, 0),
            expected
        );

        assert!(outputs.reserve_texture(key(1, 2)));
        assert!(outputs.get(first.entity).is_none());
        {
            let storage = outputs.storage();
            let slots = storage.textures.get(&first.entity).unwrap();
            assert!(!slots.writing);
            assert_eq!(slots.planes[0].as_ref().unwrap().get_pixel(0, 0), expected);
        }
        let write = outputs.prepare_texture(key(1, 2)).unwrap();
        assert!(write.publish());
        assert_eq!(
            outputs.get(first.entity).unwrap().texture().get_pixel(0, 0),
            Color::rgba(0, 0, 0, 0)
        );
    }

    #[test]
    fn a_reserved_working_set_writes_without_allocating_and_counts_both_history_slots() {
        let outputs = Outputs::<WidgetTexture>::with_budget(3 * TEXTURE_BYTES);
        let source = key(1, 1);
        let retained = key(2, 1);
        outputs.reserve(1).unwrap();
        outputs.set_retained(retained.entity, true).unwrap();
        let previous = Color::rgb(11, 29, 47);
        {
            let _batch = outputs.begin_batch();
            assert!(outputs.reserve_texture(retained));
            let mut write = outputs.prepare_texture(retained).unwrap();
            write.texture_mut().set_pixel(0, 0, &previous);
            assert!(write.publish());
        }
        let _batch = outputs.begin_batch();
        assert!(outputs.reserve_texture(source));
        assert!(outputs.reserve_texture(key(2, 2)));
        assert_eq!(outputs.storage().allocated, 3 * TEXTURE_BYTES);
        assert!(!outputs.reserve_texture(key(3, 1)));
        assert!(
            outputs
                .storage()
                .textures
                .values()
                .all(|slots| !slots.writing)
        );

        let allocations = tracked_allocations(|| {
            assert!(outputs.prepare_texture(source).unwrap().publish());
            let mut write = outputs.prepare_texture(key(2, 2)).unwrap();
            {
                let source_read = outputs.get(source.entity).unwrap();
                assert_eq!(source_read.texture().width, source.w);
                let history = outputs.previous(retained.entity).unwrap();
                write
                    .texture_mut()
                    .set_pixel(0, 0, &history.texture().get_pixel(0, 0));
            }
            assert!(write.publish());
        });
        assert_eq!(allocations, 0);
        assert_eq!(
            outputs
                .get(retained.entity)
                .unwrap()
                .texture()
                .get_pixel(0, 0),
            previous
        );
        assert_eq!(
            outputs
                .previous(retained.entity)
                .unwrap()
                .texture()
                .get_pixel(0, 0),
            previous
        );
        assert_eq!(outputs.storage().allocated, 3 * TEXTURE_BYTES);
    }

    #[test]
    fn matching_retained_reservation_does_not_enable_another_write_in_the_batch() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let input = key(1, 1);
        outputs.reserve(1).unwrap();
        outputs.set_retained(input.entity, true).unwrap();
        let _batch = outputs.begin_batch();
        assert!(outputs.reserve_texture(input));
        assert!(outputs.prepare_texture(input).unwrap().publish());
        assert!(outputs.reserve_texture(input));
        assert!(!outputs.reserve_texture(key(1, 2)));
        assert!(outputs.prepare_texture(input).is_none());
        assert!(outputs.texture_matches(input));
    }

    #[test]
    fn zero_shape_releases_retained_pixels_and_restores_without_old_history() {
        for (w, h) in [(0, 4), (4, 0)] {
            let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
            let entity = key(1, 1).entity;
            outputs.reserve(1).unwrap();
            outputs.set_retained(entity, true).unwrap();
            for generation in [1, 2] {
                let _batch = outputs.begin_batch();
                assert!(
                    outputs
                        .prepare_texture(key(1, generation))
                        .unwrap()
                        .publish()
                );
            }
            assert!(outputs.get(entity).is_some());
            assert!(outputs.previous(entity).is_some());
            assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);

            assert!(!outputs.reserve_texture(BufferKey { w, h, ..key(1, 3) }));
            assert!(outputs.get(entity).is_none());
            assert!(outputs.previous(entity).is_none());
            assert!(outputs.state(entity).unwrap().retained);
            assert_eq!(outputs.storage().allocated, 0);

            let replacement = BufferKey { w: 8, ..key(2, 1) };
            assert!(outputs.prepare_texture(replacement).unwrap().publish());
            assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
            assert!(
                outputs
                    .prepare_texture(BufferKey {
                        h: 0,
                        ..replacement
                    })
                    .is_none()
            );
            assert_eq!(outputs.storage().allocated, 0);

            let _batch = outputs.begin_batch();
            assert!(outputs.reserve_texture(key(1, 4)));
            assert!(outputs.prepare_texture(key(1, 4)).unwrap().publish());
            assert!(outputs.get(entity).is_some());
            assert!(outputs.previous(entity).is_none());
        }
    }

    #[test]
    fn zero_shape_discards_an_active_lease_without_releasing_its_live_budget() {
        for publish in [false, true] {
            let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
            let input = key(1, 1);
            outputs.reserve(1).unwrap();
            outputs.set_retained(input.entity, true).unwrap();
            {
                let _batch = outputs.begin_batch();
                assert!(outputs.prepare_texture(input).unwrap().publish());
            }
            let _batch = outputs.begin_batch();
            let pending = outputs.prepare_texture(key(1, 2)).unwrap();
            assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
            assert!(!outputs.reserve_texture(BufferKey { w: 0, ..key(1, 3) }));
            assert!(outputs.get(input.entity).is_none());
            assert!(outputs.previous(input.entity).is_none());
            assert_eq!(outputs.storage().allocated, TEXTURE_BYTES);
            assert!(!outputs.reserve_texture(key(1, 4)));
            assert!(outputs.prepare_texture(key(2, 1)).unwrap().publish());
            assert!(!outputs.reserve_texture(key(3, 1)));
            if publish {
                assert!(!pending.publish());
            } else {
                drop(pending);
            }
            assert!(outputs.get(input.entity).is_none());
            assert!(outputs.previous(input.entity).is_none());
            assert!(outputs.prepare_texture(key(3, 1)).unwrap().publish());
        }
    }

    #[test]
    fn taking_the_plan_releases_the_storage_borrow_before_production() {
        let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
        let plan = outputs.take_texture_plan().unwrap();
        assert!(outputs.take_texture_plan().is_none());
        let input = key(1, 1);
        assert!(outputs.reserve_texture(input));
        assert!(outputs.prepare_texture(input).unwrap().publish());
        assert!(outputs.get(input.entity).is_some());
        outputs.put_texture_plan(plan);
        assert!(outputs.take_texture_plan().is_some());
    }

    #[test]
    fn releasing_history_keeps_current_and_reclaims_the_spare_pixel_slot() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let entity = key(1, 1).entity;
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        for generation in [1, 2] {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(key(1, generation)).unwrap();
            write
                .texture_mut()
                .set_pixel(0, 0, &Color::rgb(10, 20, generation as u8));
            assert!(write.publish());
        }
        outputs.set_retained(entity, false).unwrap();
        outputs.reconcile_textures(|candidate| candidate == entity, |_| true);
        assert_eq!(
            outputs.get(entity).unwrap().texture().get_pixel(0, 0),
            Color::rgb(10, 20, 2)
        );
        assert!(outputs.previous(entity).is_none());
        assert!(outputs.prepare_texture(key(2, 1)).unwrap().publish());
        assert!(outputs.prepare_texture(key(1, 3)).unwrap().publish());
        assert_eq!(outputs.storage().allocated, 2 * TEXTURE_BYTES);
    }

    #[test]
    fn concurrent_write_is_rejected_without_replacing_the_active_lease() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let first_key = key(1, 1);
        let expected = Color::rgb(19, 37, 71);
        let mut first = outputs.prepare_texture(first_key).unwrap();
        first.texture_mut().set_pixel(0, 0, &expected);

        assert!(outputs.prepare_texture(key(1, 2)).is_none());
        assert!(outputs.get(first_key.entity).is_none());
        assert!(first.publish());
        assert!(outputs.texture_matches(first_key));
        assert_eq!(
            outputs
                .get(first_key.entity)
                .unwrap()
                .texture()
                .get_pixel(0, 0),
            expected
        );
    }

    #[test]
    fn trimming_an_active_write_defers_release_until_its_lease_finishes() {
        for publish in [false, true] {
            let outputs = Outputs::<WidgetTexture>::with_budget(TEXTURE_BYTES);
            let first_key = key(1, 1);
            let pending = outputs.prepare_texture(first_key).unwrap();
            outputs.trim_memory();

            assert!(outputs.get(first_key.entity).is_none());
            assert!(outputs.prepare_texture(key(2, 1)).is_none());
            if publish {
                assert!(!pending.publish());
            } else {
                drop(pending);
            }

            assert!(outputs.get(first_key.entity).is_none());
            assert!(outputs.prepare_texture(key(2, 1)).unwrap().publish());
        }
    }

    #[test]
    fn retained_output_is_written_once_per_batch() {
        let outputs = Outputs::<WidgetTexture>::with_budget(2 * TEXTURE_BYTES);
        let entity = key(1, 1).entity;
        outputs.reserve(1).unwrap();
        outputs.set_retained(entity, true).unwrap();
        let first_color = Color::rgb(200, 10, 20);
        let second_color = Color::rgb(30, 40, 220);
        {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(key(1, 1)).unwrap();
            write.texture_mut().set_pixel(0, 0, &first_color);
            assert!(write.publish());
        }
        {
            let _batch = outputs.begin_batch();
            let mut write = outputs.prepare_texture(key(1, 2)).unwrap();
            write.texture_mut().set_pixel(0, 0, &second_color);
            assert!(write.publish());

            assert!(outputs.prepare_texture(key(1, 3)).is_none());
            assert_eq!(
                outputs.get(entity).unwrap().texture().get_pixel(0, 0),
                second_color
            );
            assert_eq!(
                outputs.previous(entity).unwrap().texture().get_pixel(0, 0),
                first_color
            );
        }
        {
            let _batch = outputs.begin_batch();
            assert_eq!(
                outputs.previous(entity).unwrap().texture().get_pixel(0, 0),
                second_color
            );
        }
    }
}
