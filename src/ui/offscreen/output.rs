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
        })
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

    pub(crate) fn prepare_texture(&self, key: BufferKey) -> Option<TextureWrite<'_>> {
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
        if key.w == 0 || key.h == 0 {
            self.invalidate(key.entity);
            return None;
        }
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
        self.invalidate(key.entity);
        let state = self.state(key.entity)?;
        let slot = if state.retained {
            state
                .previous
                .map_or(0, |previous| 1 - previous.record.slot)
        } else {
            state.current.map_or(0, |current| current.record.slot)
        };
        let reusable = storage
            .textures
            .get_mut(&key.entity)
            .and_then(|slots| slots.planes[slot].take());
        let mut texture = if let Some(texture) = reusable {
            texture
        } else {
            let total = storage.allocated.checked_add(bytes)?;
            if total > storage.budget {
                return None;
            }
            storage
                .textures
                .entry(key.entity)
                .or_insert_with(|| TextureSlots {
                    planes: [None, None],
                    shape,
                    writing: false,
                    discard: false,
                });
            storage.allocated = total;
            Texture::owned(key.w, key.h, key.format)
        };
        texture.buf.as_mut_slice().fill(0);
        storage
            .textures
            .get_mut(&key.entity)
            .expect("prepared texture slots")
            .writing = true;
        drop(storage);
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
