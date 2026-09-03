//! IMAGE surface vocabulary independent of byte coding.

mod color;
mod layout;
mod memory;
mod surface;
mod view;

pub use color::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    TransferFunction,
};
pub use layout::{PlaneGeometries, PlaneGeometry, PlaneRole, SampleLayout};
pub use memory::{
    PLANE_RECORD_LEN, PlaneMemoryBuilder, PlaneMemoryError, PlaneMemoryFlags, PlaneMemoryLayout,
    PlaneMemoryRecordError,
};
pub use surface::{
    SURFACE_RECORD_LEN, SurfaceDescriptor, SurfaceError, SurfaceFlags, SurfaceRecordError,
};
pub use view::{RawImagePlane, RawImagePlanes, RawImageView, RawImageViewError};
