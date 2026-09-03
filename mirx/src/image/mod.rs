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

mod color;
mod encode;
mod layout;
mod memory;
mod plan;
mod surface;
mod view;

pub use color::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    TransferFunction,
};
pub use encode::{RawImageAsset, RawImageEncodeError};
pub use layout::{PlaneGeometries, PlaneGeometry, PlaneRole, SampleLayout};
pub use memory::{
    PLANE_RECORD_LEN, PlaneMemoryBuilder, PlaneMemoryError, PlaneMemoryFlags, PlaneMemoryLayout,
    PlaneMemoryRecordError,
};
pub use plan::{
    BufferRequirementError, BufferRequirements, SurfaceMemoryPlan, SurfaceMemoryPlanes,
    SurfacePlanError, SurfaceRequirements,
};
pub use surface::{
    SURFACE_RECORD_LEN, SurfaceDescriptor, SurfaceError, SurfaceFlags, SurfaceRecordError,
};
pub use view::{RawImagePlane, RawImagePlanes, RawImageView, RawImageViewError};
