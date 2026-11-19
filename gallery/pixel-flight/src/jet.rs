//! Static aircraft slices prepared from the indexed MIRX atlas at build time.
use mirui::types::Fixed;

use asset::{COARSE_STARTS, PIVOT_Q8, STARTS};
pub use asset::{LAYERS, LENGTH, PALETTE, WIDTH};
use mirui_pixel_flight_assets as asset;

pub(super) const PIVOT_X: Fixed = Fixed::from_ratio(PIVOT_Q8[0], 256);
pub const PIVOT: [Fixed; 2] = [
    Fixed::from_ratio(PIVOT_Q8[1], 256),
    Fixed::from_ratio(PIVOT_Q8[2], 256),
];

static VOXELS: [Voxel; asset::VOXELS.len()] = model_voxels(&asset::VOXELS);
static COARSE_VOXELS: [Voxel; asset::COARSE_VOXELS.len()] = model_voxels(&asset::COARSE_VOXELS);

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

#[derive(Clone, Copy)]
pub struct Voxel {
    pub x: u8,
    pub y: u8,
    pub color: u8,
}

const fn model_voxels<const N: usize>(samples: &[[u8; 3]; N]) -> [Voxel; N] {
    let mut voxels = [Voxel {
        x: 0,
        y: 0,
        color: 0,
    }; N];
    let mut i = 0;
    while i < N {
        let [x, y, color] = samples[i];
        voxels[i] = Voxel { x, y, color };
        i += 1;
    }
    voxels
}

impl Voxel {
    pub fn a(self) -> Fixed {
        lateral(self.x)
    }
    pub fn b(self) -> Fixed {
        longitudinal(self.y)
    }
}

pub(super) fn lateral(x: u8) -> Fixed {
    Fixed::from_int(i32::from(x)) - PIVOT_X
}

pub(super) fn longitudinal(y: u8) -> Fixed {
    PIVOT[0] - Fixed::from_int(i32::from(y))
}

pub fn slices() -> [&'static [Voxel]; LAYERS] {
    core::array::from_fn(|i| &VOXELS[STARTS[i]..STARTS[i + 1]])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Detail {
    Full,
    Coarse,
}

// Rows already follow increasing y. Only the two x extrema need extra storage.
const fn layer_widths(voxels: &[Voxel], starts: &[usize; LAYERS + 1]) -> [[[u8; 2]; 2]; LAYERS] {
    let mut widths = [[[u8::MAX, 0]; 2]; LAYERS];
    let mut layer = 0;
    while layer < LAYERS {
        let mut i = starts[layer];
        let mid = i + (starts[layer + 1] - i) / 2;
        while i < starts[layer + 1] {
            let half = (i >= mid) as usize;
            let x = voxels[i].x;
            if x < widths[layer][half][0] {
                widths[layer][half][0] = x;
            }
            if x > widths[layer][half][1] {
                widths[layer][half][1] = x;
            }
            i += 1;
        }
        layer += 1;
    }
    widths
}

impl Detail {
    pub(super) fn band_bounds(self, layer: usize, half: usize) -> Option<[[u8; 2]; 2]> {
        static FULL: [[[u8; 2]; 2]; LAYERS] = layer_widths(&VOXELS, &STARTS);
        static COARSE: [[[u8; 2]; 2]; LAYERS] = layer_widths(&COARSE_VOXELS, &COARSE_STARTS);
        let voxels = self.slice(layer);
        let (first, second) = voxels.split_at(voxels.len() / 2);
        let voxels = if half == 0 { first } else { second };
        let x = match self {
            Self::Full => FULL[layer][half],
            Self::Coarse => COARSE[layer][half],
        };
        Some([x, [voxels.first()?.y, voxels.last()?.y]])
    }

    pub(super) fn for_scale(scale: Fixed) -> Self {
        if scale > Fixed::ZERO && scale <= Fixed::from_ratio(3, 2) {
            Self::Coarse
        } else {
            Self::Full
        }
    }

    pub(super) fn cell_scale(self) -> Fixed {
        match self {
            Self::Full => Fixed::ONE,
            Self::Coarse => Fixed::from_int(2),
        }
    }

    pub(super) fn center_offset(self) -> Fixed {
        match self {
            Self::Full => Fixed::ZERO,
            Self::Coarse => Fixed::from_ratio(1, 2),
        }
    }

    pub(super) fn slice(self, layer: usize) -> &'static [Voxel] {
        match self {
            Self::Full => &VOXELS[STARTS[layer]..STARTS[layer + 1]],
            Self::Coarse => &COARSE_VOXELS[COARSE_STARTS[layer]..COARSE_STARTS[layer + 1]],
        }
    }
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
                let slice = Detail::Coarse.slice(layer);
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
                for voxel in Detail::Full.slice(layer) {
                    assert!(
                        represented
                            [usize::from(voxel.y) / 2 * (WIDTH / 2) + usize::from(voxel.x) / 2]
                    );
                }
                assert_eq!(slice.is_empty(), Detail::Full.slice(layer).is_empty());
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn detail_uses_the_projected_cell_bound() {
        for raw in -256..=512 {
            let scale = Fixed::from_ratio(raw, 256);
            let detail = Detail::for_scale(scale);
            if detail == Detail::Coarse {
                assert!(scale > Fixed::ZERO);
                assert!(scale * detail.cell_scale() <= Fixed::from_int(3));
                assert_eq!(detail.center_offset(), Fixed::from_ratio(1, 2));
            } else {
                assert!(scale <= Fixed::ZERO || scale > Fixed::from_ratio(3, 2));
                assert_eq!(detail.cell_scale(), Fixed::ONE);
                assert_eq!(detail.center_offset(), Fixed::ZERO);
            }
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
