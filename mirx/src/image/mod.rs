//! IMAGE surface vocabulary independent of byte coding.

mod color;
mod layout;

pub use color::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    TransferFunction,
};
pub use layout::{PlaneGeometries, PlaneGeometry, PlaneRole, SampleLayout};
