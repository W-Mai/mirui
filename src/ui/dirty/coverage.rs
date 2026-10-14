use crate::types::{Fixed, Rect};

/// Conservative pixels written by one node, excluding a proven untouched rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintCoverage {
    pub outer: Rect,
    pub excluded: Rect,
}

impl PaintCoverage {
    pub const EMPTY: Self = Self::solid(Rect::ZERO);

    pub const fn solid(outer: Rect) -> Self {
        Self {
            outer,
            excluded: Rect::ZERO,
        }
    }

    pub fn frame(outer: Rect, excluded: Rect) -> Self {
        let excluded = outer.intersect(&excluded).unwrap_or(Rect::ZERO);
        if excluded == outer {
            Self::EMPTY
        } else {
            Self { outer, excluded }
        }
    }

    pub fn is_empty(self) -> bool {
        self.outer.w <= Fixed::ZERO || self.outer.h <= Fixed::ZERO
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self::frame(
            self.outer.union(&other.outer),
            self.excluded
                .intersect(&other.excluded)
                .unwrap_or(Rect::ZERO),
        )
    }

    pub fn clipped(self, clip: Rect) -> Self {
        self.outer
            .intersect(&clip)
            .map_or(Self::EMPTY, |outer| Self::frame(outer, self.excluded))
    }

    pub fn pixel_aligned(self) -> Self {
        if self.is_empty() {
            return Self::EMPTY;
        }
        let (x0, y0, x1, y1) = self.outer.pixel_bounds();
        let x = self.excluded.x.ceil();
        let y = self.excluded.y.ceil();
        Self::frame(
            Rect::new(x0, y0, x1 - x0, y1 - y0),
            Rect {
                x,
                y,
                w: (self.excluded.x + self.excluded.w).floor() - x,
                h: (self.excluded.y + self.excluded.h).floor() - y,
            },
        )
    }

    pub fn regions(self) -> impl Iterator<Item = Rect> {
        let outer = self.outer;
        let hole = self.excluded;
        let rects = if hole.w <= Fixed::ZERO || hole.h <= Fixed::ZERO {
            [outer, Rect::ZERO, Rect::ZERO, Rect::ZERO]
        } else {
            [
                Rect {
                    h: hole.y - outer.y,
                    ..outer
                },
                Rect {
                    y: hole.y + hole.h,
                    h: outer.y + outer.h - hole.y - hole.h,
                    ..outer
                },
                Rect {
                    x: outer.x,
                    y: hole.y,
                    w: hole.x - outer.x,
                    h: hole.h,
                },
                Rect {
                    x: hole.x + hole.w,
                    y: hole.y,
                    w: outer.x + outer.w - hole.x - hole.w,
                    h: hole.h,
                },
            ]
        };
        rects
            .into_iter()
            .filter(|rect| rect.w > Fixed::ZERO && rect.h > Fixed::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_is_two_rectangles_and_expands_without_overlap() {
        assert_eq!(core::mem::size_of::<PaintCoverage>(), 32);
        let coverage = PaintCoverage::frame(Rect::new(0, 0, 20, 10), Rect::new(2, 2, 16, 6));
        let pieces: alloc::vec::Vec<_> = coverage.regions().collect();
        assert_eq!(pieces.len(), 4);
        for (index, rect) in pieces.iter().enumerate() {
            assert!(
                pieces[index + 1..]
                    .iter()
                    .all(|other| rect.intersect(other).is_none())
            );
        }
        assert_eq!(
            pieces
                .iter()
                .map(|r| r.w.to_int() * r.h.to_int())
                .sum::<i32>(),
            104
        );
    }

    #[test]
    fn clipping_and_rounding_never_exclude_possible_paint() {
        let original = PaintCoverage::frame(
            Rect::new(Fixed::from_f32(0.5), Fixed::from_f32(0.5), 10, 8),
            Rect::new(Fixed::from_f32(2.5), Fixed::from_f32(2.5), 6, 4),
        );
        let aligned = original.pixel_aligned();
        assert_eq!(aligned.outer, Rect::new(0, 0, 11, 9));
        assert_eq!(aligned.excluded, Rect::new(3, 3, 5, 3));
        assert!(aligned.clipped(Rect::new(3, 3, 2, 2)).is_empty());
        assert_eq!(
            aligned.union(PaintCoverage::solid(aligned.outer)).excluded,
            Rect::ZERO
        );
        assert_eq!(PaintCoverage::EMPTY.union(aligned), aligned);
    }
}
