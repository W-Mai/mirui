//! Typed properties for updating a mounted sprite.

use super::{LayerSpacing, SpriteStack};
use crate::SpriteStackAsset;
use mirui::core::cache::Handle;
use mirui::ecs::{Entity, World};
use mirui::types::Fixed;
use mirui::ui::property::{Property, PropertyChange};

pub struct SpriteAsset;

impl Property for SpriteAsset {
    type Value = Handle<SpriteStackAsset>;

    fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
        if world.get::<SpriteStack>(entity).is_none_or(|sprite| {
            sprite
                .asset
                .as_ref()
                .is_some_and(|asset| asset.ptr_eq(&value))
        }) {
            return PropertyChange::Unchanged;
        }
        world
            .get_mut::<SpriteStack>(entity)
            .expect("checked sprite")
            .asset = Some(value);
        PropertyChange::Visual
    }
}

macro_rules! sprite_field {
    ($prop:ident, $field:ident, $value:ty) => {
        pub struct $prop;
        impl Property for $prop {
            type Value = $value;
            fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
                if world
                    .get::<SpriteStack>(entity)
                    .is_none_or(|sprite| sprite.$field == value)
                {
                    return PropertyChange::Unchanged;
                }
                world
                    .get_mut::<SpriteStack>(entity)
                    .expect("checked sprite")
                    .$field = value;
                PropertyChange::Visual
            }
        }
    };
}
sprite_field!(SpriteYaw, yaw, Fixed);
sprite_field!(SpritePitch, pitch, Fixed);
sprite_field!(SpriteScale, scale, Fixed);
sprite_field!(SpriteSpacing, layer_spacing, LayerSpacing);
