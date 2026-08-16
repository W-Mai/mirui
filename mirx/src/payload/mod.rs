mod color_table;
// Shared by the typed payload codecs built on the versioned envelope.
#[allow(dead_code)]
pub(crate) mod envelope;
pub mod image;

pub use color_table::{ColorTableIter, ColorTableView};
