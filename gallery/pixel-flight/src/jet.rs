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
mod tests {
    use super::*;
    #[test]
    fn coarse_slices_partition_occupied_cells_without_allocating() {
        assert_eq!(WIDTH % 2, 0);
        assert_eq!(LENGTH % 2, 0);
        assert!(COARSE_VOXELS.len() < VOXELS.len() / 3);
        let mut cells = [0; WIDTH * LENGTH * LAYERS];
        decode(&mut cells);
        let allocations = crate::test_support::tracked_allocations(|| {
            for layer in 0..LAYERS {
                let slice = MODEL.levels()[1].layer(layer).unwrap();
                let mut represented = [false; WIDTH * LENGTH / 4];
                for voxel in slice {
                    let (x, y) = (usize::from(voxel.x), usize::from(voxel.y));
                    assert!(x < WIDTH && y < LENGTH && x % 2 == 0 && y % 2 == 0);
                    assert!(PALETTE.iter().any(|p| p[0] == voxel.color));
                    let at = y / 2 * (WIDTH / 2) + x / 2;
                    assert!(!represented[at]);
                    represented[at] = true;
                    assert!((0..4).any(|i| cells
                        [layer * WIDTH * LENGTH + (y + i / 2) * WIDTH + x + i % 2]
                        != 0));
                }
                for voxel in MODEL.levels()[0].layer(layer).unwrap() {
                    assert!(
                        represented
                            [usize::from(voxel.y) / 2 * (WIDTH / 2) + usize::from(voxel.x) / 2]
                    );
                }
                assert_eq!(
                    slice.is_empty(),
                    MODEL.levels()[0].layer(layer).unwrap().is_empty()
                );
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn detail_uses_the_projected_cell_bound() {
        for raw in -256..=512 {
            let scale = Fixed::from_ratio(raw, 256);
            let level = mirui_sprite_stack::raster::sprites::select_level(
                &MODEL,
                scale,
                Fixed::from_int(3),
            );
            assert_eq!(
                level.step(),
                if scale > Fixed::ZERO && scale <= Fixed::from_ratio(3, 2) {
                    2
                } else {
                    1
                }
            );
        }
    }

    #[test]
    fn slices_have_complete_valid_palette_coverage() {
        let mut cells = [0; WIDTH * LENGTH * LAYERS];
        decode(&mut cells);
        for i in 0..WIDTH * LENGTH {
            let top = (0..LAYERS)
                .rev()
                .map(|layer| cells[layer * WIDTH * LENGTH + i])
                .find(|value| *value != 0);
            assert_eq!(PROJECTIONS.shadow[i], u8::from(top.is_some()));
            assert_eq!(
                PROJECTIONS.top[i],
                top.map_or(0, |c| 1 + ((c - 1) >> 4).min(2))
            );
        }
        let allocated = crate::test_support::tracked_allocations(|| {
            for (layer, voxels) in slices().iter().enumerate() {
                for voxel in *voxels {
                    let i = layer * WIDTH * LENGTH
                        + usize::from(voxel.y) * WIDTH
                        + usize::from(voxel.x);
                    assert_eq!(voxel.color, cells[i]);
                }
                assert_eq!(
                    voxels.len(),
                    cells[layer * WIDTH * LENGTH..(layer + 1) * WIDTH * LENGTH]
                        .iter()
                        .filter(|c| **c != 0)
                        .count()
                );
            }
        });
        assert_eq!(allocated, 0);
        assert!(
            cells
                .iter()
                .all(|c| *c == 0 || PALETTE.iter().any(|p| p[0] == *c))
        );
        assert!(cells.iter().any(|c| *c != 0));
        assert!(cells.contains(&0));
    }
}
