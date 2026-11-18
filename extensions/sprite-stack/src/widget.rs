//! A stack of parallel sprite layers, projected into a widget rectangle.
//!
//! Register `mirui_sprite_stack::view()` with the App, load the MIRX atlas once with `SpriteStackAsset::from_mirx`, and share it through a `Handle`. Yaw and pitch are degrees. Positive pitch looks down on the stack. An omitted layer spacing uses the distance stored in the asset.
//!
//! ```no_run
//! use mirui::core::cache::Handle;
//! use mirui::prelude::*;
//! use mirui_sprite_stack::{SpriteStack, SpriteStackAsset};
//!
//! #[mirui::compose]
//! fn vehicle(car: Handle<SpriteStackAsset>, heading: Signal<Fixed>) -> Entity {
//!     ui! {
//!         SpriteStack(asset: car, yaw: ${ heading.get() }, pitch: 30, width: 96, height: 80)
//!     }
//! }
//! ```

use crate::SpriteStackAsset;
use mirui::core::cache::Handle;
use mirui::types::Fixed;
use mirui::ui::{OffscreenRender, View};

mod draw;
pub mod props;

/// Uses the asset's layer spacing until the author supplies a distance.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerSpacing(Option<Fixed>);

impl<T: Into<Fixed>> From<T> for LayerSpacing {
    fn from(value: T) -> Self {
        Self(Some(value.into()))
    }
}

/// A layered sprite. Angles use degrees and scale is logical pixels per model pixel.
#[mirui::component]
pub struct SpriteStack {
    #[property(props::SpriteAsset)]
    pub asset: Option<Handle<SpriteStackAsset>>,
    #[property(props::SpriteYaw)]
    pub yaw: Fixed,
    #[property(props::SpritePitch)]
    pub pitch: Fixed,
    #[property(props::SpriteScale)]
    pub scale: Fixed,
    #[property(props::SpriteSpacing)]
    pub layer_spacing: LayerSpacing,
}

impl Default for SpriteStack {
    fn default() -> Self {
        Self {
            asset: None,
            yaw: Fixed::ZERO,
            pitch: Fixed::from_int(30),
            scale: Fixed::ONE,
            layer_spacing: LayerSpacing::default(),
        }
    }
}

impl SpriteStack {
    pub fn new(asset: Handle<SpriteStackAsset>) -> Self {
        Self {
            asset: Some(asset),
            ..Self::default()
        }
    }
}

#[mirui::event]
fn attach(
    entity: entity!(),
    sprites: query!(SpriteStack, optional OffscreenRender),
    commands: commands!(),
) {
    if let Some((_, None)) = sprites.get(entity) {
        commands.insert(entity, OffscreenRender::default());
    }
}

#[mirui::view(component = SpriteStack, name = "SpriteStack", priority = 60, invalidation = Visual, attach = attach::callback())]
fn sprite(component: component!(), paint: paint!()) {
    if let Err(error) = draw::paint(component, paint) {
        paint.fail(error);
    }
}

pub fn view() -> View {
    sprite::view()
}
