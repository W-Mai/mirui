//! IMAGE surface vocabulary independent of byte coding.
//!
//! Logical dimensions remain unchanged when a backend needs padded storage:
//!
//! ```
//! use mirx::image::{ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements};
//!
//! let surface = SurfaceDescriptor::new(
//!     319, 181, SampleLayout::RGBA8888, ColorDescription::SRGB,
//! ).unwrap();
//! let plan = surface.memory_plan(
//!     SurfaceRequirements::new()
//!         .with_base_alignment(64)
//!         .with_plane_alignment(64)
//!         .with_width_multiple(64)
//!         .with_stride_multiple(64),
//! ).unwrap();
//! let plane = plan.plane(0).unwrap();
//! assert_eq!(surface.width(), 319);
//! assert_eq!(plane.allocation_width(), 320);
//! assert_eq!(plane.stride(), 1280);
//! assert_eq!(plan.buffer_requirements().base_alignment(), 64);
//! ```
//!
//! Borrowed surfaces can be inspected before serialization:
//!
//! ```
//! use mirx::image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor};
//!
//! let surface = SurfaceDescriptor::new(
//!     2, 2, SampleLayout::NV12, ColorDescription::BT709_YUV_LIMITED,
//! ).unwrap();
//! let y = [16; 4];
//! let uv = [128; 2];
//! let view = RawImageAsset::new(surface, &[&y, &uv]).view().unwrap();
//! assert_eq!(view.plane(0).unwrap().bytes(), y);
//! assert_eq!(view.plane(1).unwrap().memory().stride(), 2);
//! let bytes = view.encode().unwrap();
//! assert!(view.matches_payload(&bytes).unwrap());
//! ```

#[cfg(test)]
pub(crate) mod test_support;

mod borrowed;
mod color;
mod color_table;
mod coverage;
mod encode;
mod encoded;
mod grid;
mod layout;
mod memory;
mod output;
mod plan;
mod reference;
mod surface;
mod units;
mod view;

pub use borrowed::{
    ImageSource, PlaneAccessError, PlaneRows, SurfaceCopyError, SurfacePlanes, SurfaceView,
};
pub use color::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    TransferFunction,
};
pub use coverage::{CoverageBudget, CoverageError};
pub use encode::{ImageEncodeError, RawImageAsset};
pub use encoded::{
    EncodedImageAsset, EncodedImageError, EncodedImageView, ImageGroupIter, ImageGroups,
};
pub use grid::{Region, RegionError, TileGrid, TileGridError, Tiles};
pub use layout::{PlaneGeometries, PlaneGeometry, PlaneRole, SampleLayout};
pub use memory::{
    PLANE_RECORD_LEN, PlaneMemoryBuilder, PlaneMemoryError, PlaneMemoryFlags, PlaneMemoryLayout,
    PlaneMemoryRecordError,
};
pub use plan::{
    BufferRequirementError, BufferRequirements, SurfaceMemoryPlan, SurfaceMemoryPlanes,
    SurfacePlanError, SurfaceRequirements,
};
pub use reference::{ImageReadError, ImageRef};
pub use surface::{
    SURFACE_RECORD_LEN, SurfaceDescriptor, SurfaceError, SurfaceFlags, SurfaceRecordError,
};
pub use units::{
    DecodeUnitRef, DecodeUnits, DecodedUnit, GroupPlanes, GroupSelection, ReferenceMode,
    UNIT_GROUP_RECORD_LEN, UnitDecodeError, UnitDecodePlan, UnitGroup, UnitGroupBuilder,
    UnitGroupError, UnitGroupRecord, UnitGroupRecordError, UnitMemoryPlan, UnitPlane, UnitPlanes,
};
pub use view::{RawImagePlanes, RawImageView, RawImageViewError, SurfacePlane};
