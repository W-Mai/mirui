use super::PhysicalDamage;
use crate::types::PhysicalRect;
use crate::ui::render_system::DIRTY_PLAN_RECT_CAPACITY;

struct Merge {
    bounds: PhysicalRect,
    members: usize,
    count: usize,
    original_area: u32,
    extra_area: u32,
}

impl PhysicalDamage {
    pub(super) fn coalesce_redraw(&mut self) {
        const { assert!(DIRTY_PLAN_RECT_CAPACITY <= usize::BITS as usize) };
        if self.len < 2 {
            return;
        }
        self.rects[..self.len].sort_unstable_by_key(|r| (r.y(), r.x(), r.height(), r.width()));
        let mut original = [0; DIRTY_PLAN_RECT_CAPACITY];
        for (dst, rect) in original.iter_mut().zip(self.as_slice()) {
            *dst = area(*rect);
        }
        while self.len > 1 {
            let all = self.as_slice().iter().copied().reduce(union).unwrap();
            let mut best = self.merge_candidate(all, &original);
            for left in 0..self.len {
                for right in left + 1..self.len {
                    let bounds = union(self.rects[left], self.rects[right]);
                    if let Some(candidate) = self.merge_candidate(bounds, &original)
                        && best.as_ref().is_none_or(|best| {
                            u64::from(candidate.extra_area) * ((best.count - 1) as u64)
                                < u64::from(best.extra_area) * ((candidate.count - 1) as u64)
                        })
                    {
                        best = Some(candidate);
                    }
                }
            }
            let Some(best) = best else { break };
            let mut kept = 0;
            for index in 0..self.len {
                if best.members & (1 << index) == 0 {
                    self.rects[kept] = self.rects[index];
                    original[kept] = original[index];
                    kept += 1;
                }
            }
            self.rects[kept] = best.bounds;
            original[kept] = best.original_area;
            self.len = kept + 1;
            // Retained entries are already sorted; insert the merged entry in order.
            while kept > 0 {
                let key = |r: PhysicalRect| (r.y(), r.x(), r.height(), r.width());
                if key(self.rects[kept - 1]) <= key(self.rects[kept]) {
                    break;
                }
                self.rects.swap(kept - 1, kept);
                original.swap(kept - 1, kept);
                kept -= 1;
            }
        }
    }

    fn merge_candidate(
        &self,
        bounds: PhysicalRect,
        original: &[u32; DIRTY_PLAN_RECT_CAPACITY],
    ) -> Option<Merge> {
        let mut candidate = Merge {
            bounds,
            members: 0,
            count: 0,
            original_area: 0,
            extra_area: area(bounds),
        };
        for (index, rect) in self.as_slice().iter().copied().enumerate() {
            if rect.right() <= bounds.x()
                || rect.x() >= bounds.right()
                || rect.bottom() <= bounds.y()
                || rect.y() >= bounds.bottom()
            {
                continue;
            }
            if rect.x() < bounds.x()
                || rect.right() > bounds.right()
                || rect.y() < bounds.y()
                || rect.bottom() > bounds.bottom()
            {
                return None;
            }
            candidate.members |= 1 << index;
            candidate.count += 1;
            candidate.original_area += original[index];
            candidate.extra_area -= area(rect);
        }
        (candidate.count > 1
            && u64::from(area(bounds)) * 2 <= u64::from(candidate.original_area) * 3)
            .then_some(candidate)
    }
}

fn area(rect: PhysicalRect) -> u32 {
    // Disjoint target-clipped rectangles fit one u16-by-u16 pixel plane.
    // Area sums fit u32; budget and score products are widened at comparison.
    u32::from(rect.width()) * u32::from(rect.height())
}

fn union(a: PhysicalRect, b: PhysicalRect) -> PhysicalRect {
    let left = a.x().min(b.x());
    let top = a.y().min(b.y());
    PhysicalRect::new(
        left,
        top,
        a.right().max(b.right()) - left,
        a.bottom().max(b.bottom()) - top,
    )
    .expect("union of target-clipped rectangles")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Fixed, Rect, Viewport};
    use crate::ui::dirty::DirtyRegions;

    fn prepared(input: &[Rect]) -> PhysicalDamage {
        let mut plan = DirtyRegions::with_rect_capacity(DIRTY_PLAN_RECT_CAPACITY);
        plan.rects.extend_from_slice(input);
        plan.physical_redraw(Viewport::new(128, 128, Fixed::ONE), false)
    }

    #[test]
    fn nearby_motion_groups_do_not_bridge_distant_images() {
        let mut input = alloc::vec::Vec::new();
        for i in 0..3 {
            input.push(Rect::new(i * 32, i * 32, 16, 16));
            input.push(Rect::new(i * 32 + 1, i * 32 + 1, 16, 16));
        }
        let result = prepared(&input);
        assert_eq!(result.len, 3);
        assert_eq!(
            result.as_slice().iter().copied().map(area).sum::<u32>(),
            3 * 289
        );
        input.reverse();
        assert_eq!(result.as_slice(), prepared(&input).as_slice());
    }

    #[test]
    fn outline_holes_are_not_repainted() {
        let result = prepared(&[
            Rect::new(0, 0, 120, 2),
            Rect::new(0, 68, 120, 2),
            Rect::new(0, 2, 2, 66),
            Rect::new(118, 2, 2, 66),
        ]);
        assert_eq!(result.len, 4);
        assert_eq!(
            result.as_slice().iter().copied().map(area).sum::<u32>(),
            744
        );
    }

    #[test]
    fn repeated_merges_do_not_compound_area_allowance() {
        let result = prepared(&[
            Rect::new(0, 0, 10, 10),
            Rect::new(15, 0, 10, 10),
            Rect::new(40, 0, 10, 10),
        ]);
        assert_eq!(result.len, 2);
        assert_eq!(
            result.as_slice().iter().copied().map(area).sum::<u32>(),
            350
        );
    }

    #[test]
    fn full_coordinate_range_uses_wide_area_arithmetic() {
        let mut plan = PhysicalDamage {
            rects: [PhysicalRect::EMPTY; DIRTY_PLAN_RECT_CAPACITY],
            len: 2,
        };
        plan.rects[0] = PhysicalRect::new(0, 0, 32767, 65535).unwrap();
        plan.rects[1] = PhysicalRect::new(32767, 0, 32768, 65535).unwrap();
        plan.coalesce_redraw();
        assert_eq!(plan.as_slice(), &[PhysicalRect::from_size(65535, 65535)]);

        // Multiple merges must preserve the same bound as one maximal pair.
        plan.len = 4;
        plan.rects[..4].copy_from_slice(&[
            PhysicalRect::new(0, 0, 32767, 32767).unwrap(),
            PhysicalRect::new(32767, 0, 32768, 32767).unwrap(),
            PhysicalRect::new(0, 32767, 32767, 32768).unwrap(),
            PhysicalRect::new(32767, 32767, 32768, 32768).unwrap(),
        ]);
        plan.coalesce_redraw();
        assert_eq!(plan.as_slice(), &[PhysicalRect::from_size(65535, 65535)]);
    }

    #[test]
    fn grouping_preserves_coverage_disjointness_and_original_area_budget() {
        for seed in 0..128u32 {
            let mut random = seed + 1;
            let mut plan = PhysicalDamage {
                rects: [PhysicalRect::EMPTY; DIRTY_PLAN_RECT_CAPACITY],
                len: DIRTY_PLAN_RECT_CAPACITY,
            };
            for (index, rect) in plan.rects.iter_mut().enumerate() {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                *rect = PhysicalRect::new(
                    (index % 4) as u16 * 12,
                    (index / 4) as u16 * 12,
                    1 + (random % 12) as u16,
                    1 + ((random >> 8) % 12) as u16,
                )
                .unwrap();
            }
            let before = plan.rects;
            let allocations = crate::test_support::tracked_allocations(|| plan.coalesce_redraw());
            assert_eq!(allocations, 0);
            let original_area = before.iter().copied().map(area).map(u64::from).sum::<u64>();
            let after_area = plan
                .as_slice()
                .iter()
                .copied()
                .map(area)
                .map(u64::from)
                .sum::<u64>();
            assert!(after_area * 2 <= original_area * 3);
            for (index, rect) in plan.as_slice().iter().enumerate() {
                assert!(
                    plan.as_slice()[index + 1..]
                        .iter()
                        .all(|other| rect.intersection(*other).is_none())
                );
            }
            for rect in before {
                assert!(
                    plan.as_slice()
                        .iter()
                        .any(|&group| group.intersection(rect) == Some(rect))
                );
            }
        }
    }
}
