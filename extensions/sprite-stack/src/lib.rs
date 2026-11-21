//! Layered sprites stored in a MIRX image atlas.
//!
//! Register `view()` once and share the loaded `SpriteStackAsset` between widgets.
//! `yaw` and `pitch` are degrees. A positive pitch looks down onto the layers.
#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub use format::{PreparedSpriteStack, SparseLevel, StackSample};
pub use mirui_sprite_stack_format as format;

#[cfg(feature = "widget")]
mod asset;
#[cfg(feature = "widget")]
pub mod raster;
#[cfg(feature = "widget")]
mod widget;

#[cfg(feature = "widget")]
pub use asset::{SpriteStackAsset, SpriteStackError, SpriteStackReaderExt};
#[cfg(feature = "widget")]
pub use widget::props as prop;
#[cfg(feature = "widget")]
pub use widget::{LayerSpacing, SpriteStack, view};

#[cfg(feature = "widget")]
mod scene;
#[cfg(feature = "widget")]
mod terrain;
#[cfg(feature = "widget")]
pub use scene::{Geometry, Object, Pose, StackScene, StackSceneHandle, StackSceneOwned};
#[cfg(feature = "widget")]
mod stage;
#[cfg(feature = "widget")]
pub use stage::{
    StackStage, StackStageCommands, StackStageHandle, StackVisual, StackVisualInput, StageCommands,
};
#[cfg(feature = "widget")]
pub use terrain::{
    HeightField, HeightFieldHandle, HeightFieldOwned, TerrainCell, TerrainError, TerrainGrid,
};

#[cfg(feature = "widget")]
mod viewport;
#[cfg(feature = "widget")]
pub use viewport::{
    Camera, FrameSize, ObjectPass, ProjectedObjects, Resolution, SceneContent, SpriteStackPlugin,
    StackViewport, ViewProjection, ViewportCamera, ViewportContent, ViewportOverlay,
    ViewportResolution, ViewportScene, ViewportStage, WorldCamera, WorldFrame, WorldLayers,
    WorldOutput,
};
