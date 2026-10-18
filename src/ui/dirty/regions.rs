use alloc::vec::Vec;

use crate::types::{Fixed, Rect};

pub(crate) struct RegionSet<const N: usize> {
    rects: [Rect; N],
    len: usize,
}

impl<const N: usize> RegionSet<N> {
    pub const fn new() -> Self {
        Self {
            rects: [Rect::ZERO; N],
            len: 0,
        }
    }

    pub fn as_slice(&self) -> &[Rect] {
        &self.rects[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn insert(&mut self, rect: Rect) {
        if rect.w <= Fixed::ZERO || rect.h <= Fixed::ZERO {
            return;
        }
        let (x0, y0, x1, y1) = rect.pixel_bounds();
        let rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
        assert!(N > 0);
        if self.len < N {
            self.rects[self.len] = rect;
            self.len += 1;
        } else {
            let get = |index| if index == N { rect } else { self.rects[index] };
            let pair = cheapest_pair(N + 1, get);
            self.rects[pair.0] = get(pair.0).union(&get(pair.1));
            if pair.1 != N {
                self.rects[pair.1] = rect;
            }
        }
        self.merge_rectangular_unions();
    }

    fn merge_rectangular_unions(&mut self) {
        'merge: loop {
            for left in 0..self.len {
                for right in left + 1..self.len {
                    if merge_cost(self.rects[left], self.rects[right]) == 0 {
                        self.rects[left] = self.rects[left].union(&self.rects[right]);
                        self.len -= 1;
                        self.rects[right] = self.rects[self.len];
                        continue 'merge;
                    }
                }
            }
            break;
        }
    }
}

pub(super) fn normalize(input: &mut Vec<Rect>, output: &mut Vec<Rect>) {
    output.resize(output.capacity(), Rect::ZERO);
    let len = normalize_into(input, output);
    output.truncate(len);
}

pub(super) fn normalize_into(input: &mut Vec<Rect>, output: &mut [Rect]) -> usize {
    assert!(!output.is_empty() || input.is_empty());
    input.retain(|rect| rect.w > Fixed::ZERO && rect.h > Fixed::ZERO);
    for rect in input.iter_mut() {
        let (x0, y0, x1, y1) = rect.pixel_bounds();
        *rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
    }
    loop {
        if let Some(len) = scan_bands(input, output) {
            input.clear();
            return len;
        }
        // Each retry removes an input rectangle, so bounded output always terminates.
        let (left, right) = cheapest_pair(input.len(), |index| input[index]);
        let other = input.swap_remove(right);
        input[left] = input[left].union(&other);
    }
}

fn scan_bands(input: &[Rect], output: &mut [Rect]) -> Option<usize> {
    let mut len = 0;
    let Some(mut top) = input.iter().map(|rect| rect.y).min() else {
        return Some(0);
    };
    while let Some(bottom) = input
        .iter()
        .flat_map(|rect| [rect.y, rect.y + rect.h])
        .filter(|&edge| edge > top)
        .min()
    {
        let mut cursor = None;
        while let Some((left, right)) = merged_interval(input, top, bottom, cursor) {
            let rect = Rect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
            };
            if let Some(previous) = output[..len].iter_mut().find(|previous| {
                previous.x == rect.x && previous.w == rect.w && previous.y + previous.h == top
            }) {
                previous.h += rect.h;
            } else if len < output.len() {
                output[len] = rect;
                len += 1;
            } else {
                return None;
            }
            cursor = Some(right);
        }
        top = bottom;
    }
    Some(len)
}

fn merged_interval(
    input: &[Rect],
    top: Fixed,
    bottom: Fixed,
    after: Option<Fixed>,
) -> Option<(Fixed, Fixed)> {
    let active = || {
        input
            .iter()
            .filter(|rect| rect.y <= top && rect.y + rect.h >= bottom)
    };
    let left = active()
        .filter(|rect| after.is_none_or(|end| rect.x + rect.w > end))
        .map(|rect| rect.x)
        .min()?;
    let mut right = left;
    loop {
        let extended = active()
            .filter(|rect| rect.x <= right)
            .map(|rect| rect.x + rect.w)
            .max()
            .unwrap_or(right);
        if extended <= right {
            return Some((left, right));
        }
        right = extended;
    }
}

fn cheapest_pair(count: usize, get: impl Fn(usize) -> Rect) -> (usize, usize) {
    let mut best = (0, 1);
    let mut cost = u128::MAX;
    for left in 0..count {
        for right in left + 1..count {
            let a = get(left);
            let b = get(right);
            let extra = merge_cost(a, b);
            if extra < cost {
                cost = extra;
                best = (left, right);
            }
        }
    }
    best
}

fn merge_cost(a: Rect, b: Rect) -> u128 {
    area(a.union(&b)) + a.intersect(&b).map_or(0, area) - area(a) - area(b)
}

fn area(rect: Rect) -> u128 {
    let (x0, y0, x1, y1) = rect.pixel_bounds();
    (i64::from(x1) - i64::from(x0)).max(0) as u128 * (i64::from(y1) - i64::from(y0)).max(0) as u128
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(rects: &[Rect], x: i32, y: i32) -> bool {
        rects
            .iter()
            .any(|rect| rect.intersect(&Rect::new(x, y, 1, 1)).is_some())
    }

    fn disjoint(rects: &[Rect]) {
        for (index, rect) in rects.iter().enumerate() {
            assert!(
                rects[index + 1..]
                    .iter()
                    .all(|other| rect.intersect(other).is_none())
            );
        }
    }

    #[test]
    fn inline_overflow_preserves_every_input_pixel_with_bounded_storage() {
        for capacity in [1, 2, 8] {
            let check = |rects: &[Rect], inputs: &[Rect]| {
                assert!(rects.len() <= capacity);
                for y in -5..20 {
                    for x in -5..20 {
                        if contains(inputs, x, y) {
                            assert!(contains(rects, x, y));
                        }
                    }
                }
            };
            let mut one = RegionSet::<1>::new();
            let mut two = RegionSet::<2>::new();
            let mut eight = RegionSet::<8>::new();
            let inputs: Vec<_> = (0..20)
                .map(|i| Rect::new(i % 11 - 4, (i * 3) % 17 - 4, 2, 3))
                .collect();
            for (index, rect) in inputs.iter().copied().enumerate() {
                let set = match capacity {
                    1 => {
                        one.insert(rect);
                        one.as_slice()
                    }
                    2 => {
                        two.insert(rect);
                        two.as_slice()
                    }
                    _ => {
                        eight.insert(rect);
                        eight.as_slice()
                    }
                };
                check(set, &inputs[..=index]);
            }
        }
    }

    #[test]
    fn crossing_regions_preserve_the_union_without_overlapping_draws() {
        for x in -3..4 {
            for y in -3..4 {
                let original = [Rect::new(0, 0, 5, 2), Rect::new(x, y, 2, 5)];
                for reverse in [false, true] {
                    let mut input = original.to_vec();
                    if reverse {
                        input.reverse();
                    }
                    let mut output = Vec::with_capacity(8);
                    normalize(&mut input, &mut output);
                    disjoint(&output);
                    for py in -4..10 {
                        for px in -4..10 {
                            assert_eq!(contains(&original, px, py), contains(&output, px, py));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn capacity_degradation_is_bounded_and_never_loses_pixels() {
        let original = [
            Rect::new(0, 0, 9, 2),
            Rect::new(3, -2, 2, 7),
            Rect::new(20, 0, 2, 2),
        ];
        for capacity in 1..8 {
            let mut input = original.to_vec();
            let mut output = Vec::with_capacity(capacity);
            normalize(&mut input, &mut output);
            assert!(output.len() <= capacity);
            assert_eq!(output.capacity(), capacity);
            disjoint(&output);
            for y in -3..6 {
                for x in -1..23 {
                    if contains(&original, x, y) {
                        assert!(contains(&output, x, y));
                    }
                }
            }
        }
    }

    #[test]
    fn translated_outlines_keep_four_disjoint_bands() {
        for (dx, dy, expected_area) in [(1, 0, 880), (0, 1, 980)] {
            let mut input = Vec::with_capacity(8);
            for (x, y) in [(0, 0), (dx, dy)] {
                input.extend([
                    Rect::new(x, y, 120, 2),
                    Rect::new(x, y + 68, 120, 2),
                    Rect::new(x, y + 2, 2, 66),
                    Rect::new(x + 118, y + 2, 2, 66),
                ]);
            }
            let mut output = Vec::with_capacity(8);
            normalize(&mut input, &mut output);
            assert_eq!(output.len(), 4);
            assert_eq!(
                output.iter().copied().map(area).sum::<u128>(),
                expected_area
            );
            disjoint(&output);
        }
    }
}
