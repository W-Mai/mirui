use super::BufferKey;
use crate::ecs::Entity;
use crate::render::texture::{ColorFormat, Texture};
use crate::ui::outputs::{
    HistoryOutput, Output, Outputs,
    private::{HistoryKind, OutputKind},
};
use core::cell::Ref;
use hashbrown::HashMap;

/// The completed pixel output of a widget subtree.
pub struct WidgetTexture;

/// A shared borrow of completed widget pixels.
pub struct WidgetTextureRead<'a> {
    texture: Ref<'a, Texture<'static>>,
}

impl WidgetTextureRead<'_> {
    pub fn texture(&self) -> &Texture<'static> {
        &self.texture
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
        .map(|texture| WidgetTextureRead { texture })
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
        })
    }

    pub(crate) fn take_texture_plan(
        &self,
    ) -> Option<crate::ui::render_system::output_plan::TexturePlan> {
        self.storage_mut().plan.take()
    }

    pub(crate) fn put_texture_plan(
        &self,
        plan: crate::ui::render_system::output_plan::TexturePlan,
    ) {
        let mut storage = self.storage_mut();
        assert!(storage.plan.is_none(), "texture plan is already stored");
        storage.plan = Some(plan);
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
        if storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.shape != shape)
        {
            let old = storage.textures.remove(&key.entity)?;
            storage.allocated -= old
                .planes
                .iter()
                .flatten()
                .map(|plane| plane.buf.as_slice().len())
                .sum::<usize>();
            self.reset(key.entity);
        }
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
    use crate::test_support::tracked_allocations;
    use crate::types::Color;

    const TEXTURE_BYTES: usize = 4 * 4 * 4;

    fn key(id: u32, generation: u32) -> BufferKey {
        BufferKey {
            entity: Entity { id, generation: 0 },
            w: 4,
            h: 4,
            format: ColorFormat::RGBA8888,
            generation,
        }
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
