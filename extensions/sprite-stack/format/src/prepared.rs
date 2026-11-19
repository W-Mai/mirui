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
}

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
        })
    }

    pub const fn step(self) -> u16 {
        self.step
    }

    pub fn layer(self, index: usize) -> Option<&'a [StackSample]> {
        let offsets = self.offsets.get(index..)?.first_chunk::<2>()?;
        Some(&self.samples[offsets[0]..offsets[1]])
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
        let [w, h, layers] = extent;
        if w == 0
            || h == 0
            || layers == 0
            || w > 256
            || h > 256
            || spacing_q8 <= 0
            || palette.is_empty()
            || palette.len() > 256
            || levels.is_empty()
            || levels[0].step != 1
        {
            return None;
        }
        let mut i = 0;
        while i < levels.len() {
            let level = levels[i];
            if level.offsets.len() != layers as usize + 1
                || (i > 0 && levels[i - 1].step >= level.step)
            {
                return None;
            }
            let mut j = 0;
            while j < level.samples.len() {
                let s = level.samples[j];
                if s.x as u16 >= w
                    || s.y as u16 >= h
                    || (s.x as u16).rem_euclid(level.step) != 0
                    || (s.y as u16).rem_euclid(level.step) != 0
                    || s.color as usize >= palette.len()
                    || palette[s.color as usize][3] != 255
                {
                    return None;
                }
                j += 1;
            }
            i += 1;
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

#[cfg(test)]
mod tests {
    use super::*;
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
