use super::SceneCanvas;
use mirui::types::Fixed;

/// Depth queries for an effect spanning multiple tiles. Resolve every outstanding sample contained in the supplied region, including samples outside the virtual scene.
pub trait DepthQuery {
    fn next_point(&self) -> Option<[i32; 2]>;
    /// Visits pending points and their maximum relevant depth. Use MAX when all depths matter.
    fn visit_pending(&self, visit: &mut dyn FnMut([i32; 2], Fixed)) {
        if let Some(point) = self.next_point() {
            visit(point, Fixed::MAX);
        }
    }
    fn resolve(&mut self, tile: &SceneCanvas<'_, '_>);
}

/// Tests opaque coverage one row at a time. Layers mark newly covered samples in the shared row and return their count.
pub fn fully_covered(
    [left, top, right, bottom]: [i32; 4],
    scratch: &mut [u8],
    mut cover_row: impl FnMut(i32, &mut [u8]) -> usize,
) -> bool {
    if left >= right || top >= bottom {
        return false;
    }
    let width = (i64::from(right) - i64::from(left)) as usize;
    let height = (i64::from(bottom) - i64::from(top)) as usize;
    let Some(mask) = width
        .checked_mul(height)
        .and_then(|len| scratch.get_mut(..len))
    else {
        return false;
    };
    (top..bottom)
        .zip(mask.chunks_exact_mut(width))
        .all(|(y, row)| {
            row.fill(0);
            cover_row(y, row) == width
        })
}

/// Visibility of up to sixteen effect samples accumulated across disjoint depth tiles.
#[derive(Clone, Copy, Default)]
pub struct DepthSamples<const N: usize> {
    resolved: u16,
    visible: u16,
}

impl<const N: usize> DepthSamples<N> {
    const MASK: u16 = {
        assert!(N <= u16::BITS as usize);
        ((1u32 << N) - 1) as u16
    };

    /// Queries only outstanding samples. A positive bias keeps a surface-bound effect visible.
    pub fn resolve(
        &mut self,
        tile: &SceneCanvas<'_, '_>,
        bias: Fixed,
        mut sample: impl FnMut(usize) -> (i32, i32, Fixed),
    ) {
        for i in self.missing() {
            let (x, y, at) = sample(i);
            if let Some(surface) = tile.sample_depth(x, y) {
                let bit = 1 << i;
                self.resolved |= bit;
                if surface >= at - bias {
                    self.visible |= bit;
                }
            }
        }
    }
    pub fn next_missing(self) -> Option<usize> {
        let missing = Self::MASK & !self.resolved;
        (missing != 0).then(|| missing.trailing_zeros() as usize)
    }
    pub fn missing(self) -> impl Iterator<Item = usize> {
        let mut mask = Self::MASK & !self.resolved;
        core::iter::from_fn(move || {
            if mask == 0 {
                return None;
            }
            let index = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            Some(index)
        })
    }
    pub fn resolved_count(self) -> u32 {
        self.resolved.count_ones()
    }
    pub fn visibility(self) -> Option<u16> {
        (self.resolved == Self::MASK).then_some(self.visible)
    }
}
