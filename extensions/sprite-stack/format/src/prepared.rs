//! Borrowed sparse slices for preprocessed indexed sprite stacks.

/// Coordinates are in base-level cells, including at coarser resolutions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackSample {
    pub x: u8,
    pub y: u8,
    pub color: u8,
}

/// Layer boundaries index the shared sample array, including empty layers.
#[derive(Clone, Copy, Debug)]
pub struct SparseLevel<'a> {
    step: u16,
    offsets: &'a [usize],
    samples: &'a [StackSample],
    bounds: Option<&'a [LayerBounds]>,
}

// Inclusive sample-coordinate bounds for the two halves of a sparse layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LayerBounds([[[u8; 2]; 2]; 2]);

impl<'a> SparseLevel<'a> {
    pub const fn new(step: u16, offsets: &'a [usize], samples: &'a [StackSample]) -> Option<Self> {
        if step == 0
            || offsets.len() < 2
            || offsets[0] != 0
            || offsets[offsets.len() - 1] != samples.len()
        {
            return None;
        }
        let mut i = 1;
        while i < offsets.len() {
            if offsets[i - 1] > offsets[i] {
                return None;
            }
            i += 1;
        }
        Some(Self {
            step,
            offsets,
            samples,
            bounds: None,
        })
    }

    pub const fn step(self) -> u16 {
        self.step
    }

    pub const fn layer_count(self) -> usize {
        self.offsets.len() - 1
    }

    pub fn layer(self, index: usize) -> Option<&'a [StackSample]> {
        let offsets = self.offsets.get(index..)?.first_chunk::<2>()?;
        Some(&self.samples[offsets[0]..offsets[1]])
    }

    const fn measure_bounds(self, layer: usize) -> LayerBounds {
        let mut halves = [[[u8::MAX, 0]; 2]; 2];
        let mut i = self.offsets[layer];
        let end = self.offsets[layer + 1];
        let mid = i + (end - i) / 2;
        while i < end {
            let p = self.samples[i];
            let b = &mut halves[(i >= mid) as usize];
            if p.x < b[0][0] {
                b[0][0] = p.x;
            }
            if p.x > b[0][1] {
                b[0][1] = p.x;
            }
            if p.y < b[1][0] {
                b[1][0] = p.y;
            }
            if p.y > b[1][1] {
                b[1][1] = p.y;
            }
            i += 1;
        }
        LayerBounds(halves)
    }

    /// Computes a read-only spatial index for static assets. N must match the layer count.
    pub const fn indexed<const N: usize>(self) -> IndexedLevel<'a, N> {
        assert!(N == self.offsets.len() - 1, "incorrect layer count");
        let mut bounds = [LayerBounds([[[0; 2]; 2]; 2]); N];
        let mut i = 0;
        while i < N {
            bounds[i] = self.measure_bounds(i);
            i += 1;
        }
        IndexedLevel {
            source: self,
            bounds,
        }
    }

    /// Returns the two sample bounds, using the precomputed index when present. An empty half has its minimum greater than its maximum.
    pub fn layer_bounds(self, layer: usize) -> Option<[[[u8; 2]; 2]; 2]> {
        if layer >= self.offsets.len() - 1 {
            return None;
        }
        Some(
            self.bounds
                .map_or_else(|| self.measure_bounds(layer), |bounds| bounds[layer])
                .0,
        )
    }
}

/// A borrowed sparse level with its spatial index stored inline.
pub struct IndexedLevel<'a, const N: usize> {
    source: SparseLevel<'a>,
    bounds: [LayerBounds; N],
}

impl<const N: usize> IndexedLevel<'_, N> {
    pub const fn level(&self) -> SparseLevel<'_> {
        SparseLevel {
            bounds: Some(&self.bounds),
            ..self.source
        }
    }
}

/// Indexed geometry shared by multiple instances. The pivot is (x, y, layer)
/// in Q24.8 base cells. Layer spacing is positive Q24.8 in the same units.
#[derive(Debug)]
pub struct PreparedSpriteStack<'a> {
    extent: [u16; 3],
    pivot_q8: [i32; 3],
    spacing_q8: i32,
    palette: &'a [[u8; 4]],
    levels: &'a [SparseLevel<'a>],
}

impl<'a> PreparedSpriteStack<'a> {
    /// Validates the whole table once. Static definitions may evaluate this at compile time.
    pub const fn new(
        extent: [u16; 3],
        pivot_q8: [i32; 3],
        spacing_q8: i32,
        palette: &'a [[u8; 4]],
        levels: &'a [SparseLevel<'a>],
    ) -> Option<Self> {
        if !valid_header(extent, spacing_q8, palette, levels.len()) {
            return None;
        }
        let mut previous_step = 0;
        let mut index = 0;
        while index < levels.len() {
            let level = levels[index];
            if !valid_level(extent, palette, level, previous_step) {
                return None;
            }
            previous_step = level.step;
            index += 1;
        }
        Some(Self {
            extent,
            pivot_q8,
            spacing_q8,
            palette,
            levels,
        })
    }
    pub const fn extent(&self) -> [u16; 3] {
        self.extent
    }
    pub const fn pivot_q8(&self) -> [i32; 3] {
        self.pivot_q8
    }
    pub const fn spacing_q8(&self) -> i32 {
        self.spacing_q8
    }
    pub const fn palette(&self) -> &'a [[u8; 4]] {
        self.palette
    }
    pub const fn levels(&self) -> &'a [SparseLevel<'a>] {
        self.levels
    }
}

const fn valid_header(
    [width, height, layers]: [u16; 3],
    spacing_q8: i32,
    palette: &[[u8; 4]],
    level_count: usize,
) -> bool {
    width > 0
        && height > 0
        && layers > 0
        && width <= 256
        && height <= 256
        && spacing_q8 > 0
        && !palette.is_empty()
        && palette.len() <= 256
        && level_count > 0
}

const fn valid_level(
    [width, height, layers]: [u16; 3],
    palette: &[[u8; 4]],
    level: SparseLevel<'_>,
    previous_step: u16,
) -> bool {
    if level.layer_count() != layers as usize
        || level.step <= previous_step
        || (previous_step == 0 && level.step != 1)
    {
        return false;
    }
    let mut index = 0;
    while index < level.samples.len() {
        let sample = level.samples[index];
        if sample.x as u16 >= width
            || sample.y as u16 >= height
            || (sample.x as u16).rem_euclid(level.step) != 0
            || (sample.y as u16).rem_euclid(level.step) != 0
            || sample.color as usize >= palette.len()
            || palette[sample.color as usize][3] != 255
        {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_index_matches_unsorted_samples_and_empty_layers() {
        let samples = [
            StackSample {
                x: 11,
                y: 7,
                color: 0,
            },
            StackSample {
                x: 2,
                y: 4,
                color: 0,
            },
            StackSample {
                x: 9,
                y: 1,
                color: 0,
            },
            StackSample {
                x: 5,
                y: 6,
                color: 0,
            },
        ];
        let level = SparseLevel::new(1, &[0, 0, 1, 4], &samples).unwrap();
        let index = level.indexed::<3>();
        for layer in 0..4 {
            assert_eq!(index.level().layer_bounds(layer), level.layer_bounds(layer));
        }
        assert_eq!(level.layer_bounds(1).unwrap()[0], [[255, 0]; 2]);
        assert_eq!(
            level.layer_bounds(2).unwrap(),
            [[[2, 2], [4, 4]], [[5, 9], [1, 6]]]
        );
        assert_eq!(level.layer_bounds(3), None);
    }

    #[test]
    fn sparse_levels_validate_bounds_before_rendering() {
        let sample = [StackSample {
            x: 1,
            y: 0,
            color: 0,
        }];
        let palette = [[0, 0, 0, 255]];
        assert!(SparseLevel::new(1, &[0, 2, 1], &sample).is_none());
        let base = SparseLevel::new(1, &[0, 0, 1], &sample).unwrap();
        assert_eq!(base.layer(0), Some([].as_slice()));
        assert_eq!(base.layer(usize::MAX), None);
        let coarse = SparseLevel::new(2, &[0, 0, 1], &sample).unwrap();
        assert!(PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &palette, &[base]).is_some());
        assert!(
            PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &palette, &[base, coarse]).is_none()
        );
        assert!(PreparedSpriteStack::new([1, 1, 2], [0; 3], 256, &palette, &[base]).is_none());
        assert!(PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &[[0; 4]], &[base]).is_none());
    }
}
