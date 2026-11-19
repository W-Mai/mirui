use crate::{HeightField, HeightFieldHandle, SpriteStackAsset, TerrainError};
use mirui::{
    core::{
        cache::Handle,
        model::{CollectionError, ModelHandle, ModelItemId, ModelList, OwnedModel},
    },
    types::Fixed,
};

/// Identifies one instance for its lifetime in its owning scene.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Object(ModelItemId);

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
    Field(Handle<HeightField>),
    LiveField(HeightFieldHandle),
}

impl From<Handle<SpriteStackAsset>> for Geometry {
    fn from(s: Handle<SpriteStackAsset>) -> Self {
        Self::Sprite(s)
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
            Self::Sprite(_) => None,
        }
    }
}

#[mirui::model]
struct Instance {
    geometry: Geometry,
    pose: Pose,
}

#[mirui::model]
impl Instance {}

struct Objects(ModelList<Instance>);

/// Object identity, order, and transforms for one shared game simulation.
#[mirui::model]
pub struct StackScene {
    entries: Objects,
    revision: u64,
}

impl Default for StackScene {
    fn default() -> Self {
        Self::new()
    }
}

impl StackScene {
    pub fn new() -> Self {
        Self {
            entries: Objects(ModelList::new()),
            revision: 0,
        }
    }
    /// Prepares all instance slots. Subsequent placement and movement reuse their storage.
    pub fn bounded(limit: usize) -> Self {
        Self {
            entries: Objects(ModelList::bounded(limit)),
            revision: 0,
        }
    }

    pub fn place(
        &mut self,
        geometry: impl Into<Geometry>,
        pose: Pose,
    ) -> Result<Object, CollectionError> {
        if self.entries.0.limit().is_none() && self.entries.0.capacity() == self.entries.0.len() {
            self.entries.0.reserve(1)?;
        }
        let id = self
            .entries
            .0
            .try_push(Instance {
                geometry: geometry.into(),
                pose,
            })
            .map_err(|(err, _)| err)?;
        self.revision = self.revision.wrapping_add(1);
        Ok(Object(id))
    }

    /// Visits shared geometry in insertion order without copying the assets.
    pub fn for_each(&self, mut visit: impl FnMut(Object, &Geometry, Pose)) {
        for (id, value) in self.entries.0.entries() {
            value.read(|entry| visit(Object(id), &entry.geometry, entry.pose));
        }
    }
}

#[mirui::model]
impl StackScene {
    #[observe]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn place_into(
        &mut self,
        geometry: Geometry,
        pose: Pose,
        result: &mut Option<Object>,
    ) -> Result<(), CollectionError> {
        *result = Some(self.place(geometry, pose)?);
        Ok(())
    }

    pub fn remove(&mut self, id: Object) -> Result<(), CollectionError> {
        self.entries
            .0
            .remove(id.0)
            .ok_or(CollectionError::Missing)?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn pose(&self, id: Object) -> Option<Pose> {
        self.entries
            .0
            .get_by_id(id.0)
            .map(|value| value.read(|v| v.pose))
    }

    pub fn set_pose(&mut self, id: Object, pose: Pose) -> Result<(), CollectionError> {
        let entry = self
            .entries
            .0
            .get_by_id_mut(id.0)
            .ok_or(CollectionError::Missing)?;
        if entry.read(|v| v.pose) != pose {
            entry.update(|v| v.pose = pose);
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }

    pub fn move_forward(&mut self, id: Object, distance: Fixed) -> Result<(), CollectionError> {
        let mut pose = self.pose(id).ok_or(CollectionError::Missing)?;
        pose.position[0] += Fixed::sin_deg(pose.yaw) * distance;
        pose.position[2] += Fixed::cos_deg(pose.yaw) * distance;
        self.set_pose(id, pose)
    }

    /// Queries a particular placed field. Finite grids return None beyond their edges.
    pub fn height_at(&self, id: Object, x: Fixed, z: Fixed) -> Result<Option<Fixed>, TerrainError> {
        let entry = self
            .entries
            .0
            .get_by_id(id.0)
            .ok_or(TerrainError::MissingObject)?;
        entry.read(|v| {
            let p = v.pose;
            if !p.pitch.is_zero() || !p.roll.is_zero() {
                return Err(TerrainError::Tilted);
            }
            let (x, z) = (x - p.position[0], z - p.position[2]);
            let (x, z) = if p.yaw.is_zero() {
                (x, z)
            } else {
                let (s, c) = (Fixed::sin_deg(p.yaw), Fixed::cos_deg(p.yaw));
                (x * c - z * s, x * s + z * c)
            };
            v.geometry
                .with_field(|field| field.height_at(x, z).map(|y| y + p.position[1]))
                .ok_or(TerrainError::NotHeightField)
        })
    }
}

impl StackSceneHandle {
    pub fn place(
        &self,
        geometry: impl Into<Geometry>,
        pose: Pose,
    ) -> Result<Object, CollectionError> {
        let mut result = None;
        self.place_into(geometry.into(), pose, &mut result)?;
        Ok(result.expect("successful placement yields an object"))
    }
}

impl StackSceneOwned {
    pub fn place(
        &mut self,
        geometry: impl Into<Geometry>,
        pose: Pose,
    ) -> Result<Object, CollectionError> {
        let mut result = None;
        self.place_into(geometry.into(), pose, &mut result)?;
        Ok(result.expect("successful placement yields an object"))
    }
}
