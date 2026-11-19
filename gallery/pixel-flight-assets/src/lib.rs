//! Sparse aircraft samples in layer order. Each triplet contains x, y and the material palette index.
#![no_std]

include!(concat!(env!("OUT_DIR"), "/pixel_flight_jet.rs"));

pub use mirui_sprite_stack_format::{PreparedSpriteStack, SparseLevel, StackSample};

const fn rgba_palette() -> [[u8; 4]; 65] {
    let mut rgba = [[0; 4]; 65];
    let mut index = 0;
    while index < PALETTE.len() {
        let [color, r, g, b, _] = PALETTE[index];
        rgba[color as usize] = [r, g, b, 255];
        index += 1;
    }
    rgba
}

/// Both detail levels reference the samples generated from the same indexed MIRX file.
pub static MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
    [WIDTH as u16, LENGTH as u16, LAYERS as u16],
    PIVOT_Q8,
    256,
    &rgba_palette(),
    &[
        SparseLevel::new(1, &STARTS, &VOXELS).expect("invalid aircraft slices"),
        SparseLevel::new(2, &COARSE_STARTS, &COARSE_VOXELS).expect("invalid coarse slices"),
    ],
)
.expect("invalid aircraft model");
