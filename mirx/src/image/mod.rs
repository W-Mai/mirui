//! IMAGE surface vocabulary independent of byte coding.

mod color;
mod layout;
mod surface;

pub use color::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    TransferFunction,
};
pub use layout::{PlaneGeometries, PlaneGeometry, PlaneRole, SampleLayout};
pub use surface::{
    SURFACE_RECORD_LEN, SurfaceDescriptor, SurfaceError, SurfaceFlags, SurfaceRecordError,
};
