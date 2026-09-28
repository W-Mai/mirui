mod model;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use model::PixelModel;
#[allow(unused_imports)]
pub(crate) use types::{
    COLOR_COUNT, FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelFrames, PixelModal, PixelTool,
};
