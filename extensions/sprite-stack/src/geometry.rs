use crate::{GeneratedStackAsset, HeightField, HeightFieldHandle, SpriteStackAsset};
use mirui::{
    core::{cache::Handle, model::ModelHandle},
    types::Fixed,
};

/// Y points up. Yaw is measured around Y, pitch around X and roll around Z, in degrees.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pose {
    pub position: [Fixed; 3],
    pub yaw: Fixed,
    pub pitch: Fixed,
    pub roll: Fixed,
}

impl Pose {
    pub fn at(x: impl Into<Fixed>, y: impl Into<Fixed>, z: impl Into<Fixed>) -> Self {
        Self {
            position: [x.into(), y.into(), z.into()],
            ..Self::default()
        }
    }
}

/// Geometry shared by the game and every view of an instance.
pub enum Geometry {
    Sprite(Handle<SpriteStackAsset>),
    Prepared(&'static crate::PreparedSpriteStack<'static>),
    /// Samples projected cells of a sparse model and depth-tests them against other scene objects.
    Sampled(&'static crate::PreparedSpriteStack<'static>),
    Generated(GeneratedStackAsset),
    Field(Handle<HeightField>),
    LiveField(HeightFieldHandle),
}

impl From<&'static crate::PreparedSpriteStack<'static>> for Geometry {
    fn from(s: &'static crate::PreparedSpriteStack<'static>) -> Self {
        Self::Prepared(s)
    }
}

impl From<Handle<SpriteStackAsset>> for Geometry {
    fn from(s: Handle<SpriteStackAsset>) -> Self {
        Self::Sprite(s)
    }
}

impl From<GeneratedStackAsset> for Geometry {
    fn from(asset: GeneratedStackAsset) -> Self {
        Self::Generated(asset)
    }
}
impl From<Handle<HeightField>> for Geometry {
    fn from(f: Handle<HeightField>) -> Self {
        Self::Field(f)
    }
}
impl From<HeightField> for Geometry {
    fn from(f: HeightField) -> Self {
        Self::Field(Handle::new(f))
    }
}
impl From<HeightFieldHandle> for Geometry {
    fn from(f: HeightFieldHandle) -> Self {
        Self::LiveField(f)
    }
}

impl Geometry {
    pub fn with_field<R>(&self, read: impl FnOnce(&HeightField) -> R) -> Option<R> {
        match self {
            Self::Field(field) => Some(read(field.get())),
            Self::LiveField(field) => Some(field.read(read)),
            Self::Sprite(_) | Self::Prepared(_) | Self::Sampled(_) | Self::Generated(_) => None,
        }
    }
}
