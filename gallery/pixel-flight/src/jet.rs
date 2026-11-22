//! Static aircraft slices prepared from the indexed MIRX atlas at build time.
use mirui::types::Fixed;

use baked::{COARSE_STARTS, COARSE_VOXELS, PIVOT_Q8, STARTS, VOXELS};
pub use baked::{LAYERS, LENGTH, PALETTE, WIDTH};
pub use mirui_sprite_stack::StackSample as Voxel;
use mirui_sprite_stack::{PreparedSpriteStack, SparseLevel};

mod baked {
    use mirui_sprite_stack::StackSample;

    include!(concat!(env!("OUT_DIR"), "/pixel_flight_jet.rs"));
}

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

const FINE: mirui_sprite_stack::format::IndexedLevel<'static, LAYERS> =
    SparseLevel::new(1, &STARTS, &VOXELS)
        .expect("invalid aircraft slices")
        .indexed();
const COARSE: mirui_sprite_stack::format::IndexedLevel<'static, LAYERS> =
    SparseLevel::new(2, &COARSE_STARTS, &COARSE_VOXELS)
        .expect("invalid coarse slices")
        .indexed();

/// Both detail levels reference the samples generated from the same indexed MIRX file.
pub static MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
    [WIDTH as u16, LENGTH as u16, LAYERS as u16],
    PIVOT_Q8,
    256,
    &rgba_palette(),
    &[FINE.level(), COARSE.level()],
)
.expect("invalid aircraft model");

pub(super) const PIVOT_X: Fixed = Fixed::from_ratio(PIVOT_Q8[0], 256);
pub const PIVOT: [Fixed; 2] = [
    Fixed::from_ratio(PIVOT_Q8[1], 256),
    Fixed::from_ratio(PIVOT_Q8[2], 256),
];

pub(super) fn nozzle(side: Fixed) -> [Fixed; 3] {
    [
        side * Fixed::from_ratio(196, 100),
        PIVOT[0] - Fixed::from_ratio(436, 10),
        Fixed::from_ratio(41, 10) - PIVOT[1],
    ]
}

/// Expands the full-detail slices into caller-owned material indices.
pub fn decode(out: &mut [u8; WIDTH * LENGTH * LAYERS]) {
    out.fill(0);
    for (layer, voxels) in slices().into_iter().enumerate() {
        for voxel in voxels {
            out[(layer * LENGTH + usize::from(voxel.y)) * WIDTH + usize::from(voxel.x)] =
                voxel.color;
        }
    }
}

pub fn slices() -> [&'static [Voxel]; LAYERS] {
    core::array::from_fn(|i| MODEL.levels()[0].layer(i).expect("valid aircraft layer"))
}

pub(super) struct Projections {
    pub shadow: [u8; WIDTH * LENGTH],
    pub top: [u8; WIDTH * LENGTH],
}

pub(super) static PROJECTIONS: Projections = {
    let mut out = Projections {
        shadow: [0; WIDTH * LENGTH],
        top: [0; WIDTH * LENGTH],
    };
    let mut i = VOXELS.len();
    while i > 0 {
        i -= 1;
        let voxel = &VOXELS[i];
        let at = voxel.y as usize * WIDTH + voxel.x as usize;
        out.shadow[at] = 1;
        if out.top[at] == 0 {
            let color = (voxel.color - 1) >> 4;
            out.top[at] = 1 + if color > 2 { 2 } else { color };
        }
    }
    out
};
#[cfg(test)]
#[path = "../tests/unit/jet.rs"]
mod tests;
