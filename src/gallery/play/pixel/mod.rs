mod model;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use model::PixelModel;
#[cfg(feature = "persistence")]
pub(crate) use model::{PixelSave, SAVE_LEN};
#[allow(unused_imports)]
pub(crate) use types::{
    COLOR_COUNT, FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelClipboardStatus, PixelFrame,
    PixelFrames, PixelModal, PixelTool,
};
