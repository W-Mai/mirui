//! Layered sprites stored in a MIRX image atlas.
//!
//! Register `view()` once and share the loaded `SpriteStackAsset` between widgets.
//! `yaw` and `pitch` are degrees. A positive pitch looks down onto the layers.
#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;
#[cfg(all(test, feature = "widget"))]
#[path = "../tests/support/allocations.rs"]
mod allocations;

pub use format::{PreparedSpriteStack, SparseLevel, StackSample};
pub use mirui_sprite_stack_format as format;

#[cfg(feature = "widget")]
mod asset;
#[cfg(feature = "widget")]
mod generated;
#[cfg(feature = "widget")]
pub mod raster;
#[cfg(feature = "widget")]
pub use raster::SolidStack;
#[cfg(feature = "widget")]
mod widget;

#[cfg(feature = "widget")]
pub use asset::{SpriteStackAsset, SpriteStackError, SpriteStackReaderExt};
#[cfg(feature = "widget")]
pub use generated::{GeneratedError, GeneratedLevel, GeneratedStackAsset};
#[cfg(feature = "widget")]
pub use widget::props as prop;
#[cfg(feature = "widget")]
pub use widget::{LayerSpacing, SpriteStack, view};

#[cfg(feature = "widget")]
mod geometry;
#[cfg(feature = "widget")]
mod terrain;
#[cfg(feature = "widget")]
pub use geometry::{Geometry, Pose};
#[cfg(feature = "widget")]
mod stage;
#[cfg(feature = "widget")]
pub use stage::{StackStage, StackStageCommands, StackStageHandle, StageCommands};
#[cfg(feature = "widget")]
mod visual;
#[cfg(feature = "widget")]
pub use terrain::{
    HeightField, HeightFieldHandle, HeightFieldOwned, TerrainCell, TerrainError, TerrainGrid,
};
#[cfg(feature = "widget")]
pub use visual::{LayerEntrance, SampledStyle, SampledVisual, StackVisual, StackVisualInput};

#[cfg(feature = "widget")]
mod viewport;
#[cfg(feature = "widget")]
pub use viewport::{
    Camera, ContentFrame, FrameContent, FrameSize, Resolution, SpriteStackPlugin, StackViewport,
    TerrainMember, TerrainTarget, ViewProjection, ViewportCamera, ViewportContent, ViewportOutput,
    ViewportOverlay, ViewportResolution, ViewportStage, WorldCamera,
};
