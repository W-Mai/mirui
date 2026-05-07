use alloc::vec::Vec;

/// Image component — raw RGBA pixel data
pub struct Image {
    pub data: Vec<u8>, // RGBA
    pub width: u16,
    pub height: u16,
}

impl Image {
    pub fn new(data: Vec<u8>, width: u16, height: u16) -> Self {
        Self {
            data,
            width,
            height,
        }
    }
}
