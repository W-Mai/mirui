use core::ops::Range;

use crate::types::Fixed;
use crate::types::fixed::storage;

#[derive(Clone, Copy)]
pub(super) struct ItemGeometry {
    count: u32,
    extent: i64,
}

impl ItemGeometry {
    pub(super) fn new(count: u32, extent: Fixed) -> Option<Self> {
        let extent = i64::from(storage::to_i32(extent));
        if extent <= 0 || extent * i64::from(count) > i64::from(storage::to_i32(Fixed::MAX)) {
            return None;
        }
        Some(Self { count, extent })
    }

    pub(super) fn visible(self, offset: Fixed, viewport: Fixed) -> Range<u32> {
        let offset = i64::from(storage::to_i32(offset));
        let viewport = i64::from(storage::to_i32(viewport));
        let content = self.extent * i64::from(self.count);
        let start = offset.clamp(0, content);
        let end = (offset + viewport.max(0)).clamp(0, content);
        let first = (start / self.extent) as u32;
        if start >= end {
            return first..first;
        }
        first..((end + self.extent - 1) / self.extent) as u32
    }

    pub(super) fn retained(self, visible: Range<u32>, capacity: u32) -> Range<u32> {
        if visible.is_empty() {
            return visible;
        }
        const OVERSCAN: u32 = 2;
        let visible_count = visible.end - visible.start;
        let spare = capacity.saturating_sub(visible_count);
        let before = OVERSCAN.min(visible.start).min(spare);
        let after = OVERSCAN.min(self.count - visible.end).min(spare - before);
        visible.start - before..visible.start + visible_count.min(capacity) + after
    }

    pub(super) fn capacity(self, viewport: Fixed) -> u32 {
        let viewport = i64::from(storage::to_i32(viewport));
        if viewport <= 0 {
            return 0;
        }
        (((viewport + self.extent - 1) / self.extent + 5) as u32).min(self.count)
    }

    pub(super) fn position(self, index: u32) -> Fixed {
        debug_assert!(index <= self.count);
        Fixed::from_ratio(
            (self.extent * i64::from(index)) as i32,
            storage::to_i32(Fixed::ONE),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_interval_preserves_partial_items_at_fixed_precision() {
        let geometry = ItemGeometry::new(100, Fixed::from_int(40)).unwrap();
        let unit = Fixed::from_ratio(1, 256);
        for (offset, expected) in [
            (Fixed::ZERO, 0..2),
            (Fixed::from_int(20), 0..3),
            (Fixed::from_int(40) - unit, 0..3),
            (Fixed::from_int(40), 1..3),
            (Fixed::from_int(40) + unit, 1..4),
            (Fixed::from_int(-20), 0..2),
            (Fixed::from_int(-80), 0..0),
            (Fixed::from_int(3980), 99..100),
            (Fixed::from_int(4000), 100..100),
        ] {
            assert_eq!(geometry.visible(offset, Fixed::from_int(80)), expected);
        }
        assert_eq!(geometry.visible(Fixed::from_int(20), Fixed::ZERO), 0..0);
        assert_eq!(geometry.visible(Fixed::MAX, Fixed::MAX), 100..100);
    }

    #[test]
    fn visible_interval_matches_positive_intersection() {
        for count in 0..8 {
            for extent_units in [1, 3, 256, 1025] {
                let extent = Fixed::from_ratio(extent_units, 256);
                let geometry = ItemGeometry::new(count, extent).unwrap();
                for offset_units in -2 * extent_units..10 * extent_units {
                    for viewport_units in [0, 1, extent_units, 2 * extent_units] {
                        let visible = geometry.visible(
                            Fixed::from_ratio(offset_units, 256),
                            Fixed::from_ratio(viewport_units, 256),
                        );
                        for index in 0..count {
                            let start = index as i32 * extent_units;
                            let end = start + extent_units;
                            let intersects = viewport_units > 0
                                && start < offset_units + viewport_units
                                && end > offset_units;
                            assert_eq!(visible.contains(&index), intersects);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn overscan_never_displaces_visible_items_or_fills_past_data() {
        let geometry = ItemGeometry::new(100, Fixed::from_int(40)).unwrap();
        assert_eq!(geometry.retained(0..2, 8), 0..4);
        assert_eq!(geometry.retained(20..23, 3), 20..23);
        assert_eq!(geometry.retained(20..23, 6), 18..24);
        assert_eq!(geometry.retained(99..100, 12), 97..100);
        assert_eq!(geometry.retained(100..100, 12), 100..100);
    }

    #[test]
    fn content_extent_and_positions_must_fit_layout_coordinates() {
        assert!(ItemGeometry::new(1, Fixed::ZERO).is_none());
        assert!(ItemGeometry::new(1, -Fixed::ONE).is_none());
        assert!(ItemGeometry::new(u32::MAX, Fixed::ONE).is_none());
        let geometry = ItemGeometry::new(1, Fixed::MAX).unwrap();
        assert_eq!(geometry.position(1), Fixed::MAX);
        let geometry = ItemGeometry::new(65_536, Fixed::from_int(32)).unwrap();
        assert_eq!(geometry.position(65_535), Fixed::from_int(2_097_120));
    }
}
