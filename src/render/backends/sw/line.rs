use super::SwRenderer;
use crate::types::{Color, Fixed, Fixed64, Point, Rect};

impl SwRenderer<'_> {
    pub(super) fn try_aligned_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opacity: u8,
    ) -> bool {
        if width <= Fixed::ZERO || p1 == p2 || !self.scratch.clip_stack.is_empty() {
            return false;
        }
        let half = Fixed64::from_fixed(width / 2);
        let (x, y, w, h) = if p1.x == p2.x {
            (
                Fixed64::from_fixed(p1.x) - half,
                Fixed64::from_fixed(p1.y.min(p2.y)),
                Fixed64::from_fixed(width),
                Fixed64::from_fixed(p1.y.max(p2.y)) - Fixed64::from_fixed(p1.y.min(p2.y)),
            )
        } else if p1.y == p2.y {
            (
                Fixed64::from_fixed(p1.x.min(p2.x)),
                Fixed64::from_fixed(p1.y) - half,
                Fixed64::from_fixed(p1.x.max(p2.x)) - Fixed64::from_fixed(p1.x.min(p2.x)),
                Fixed64::from_fixed(width),
            )
        } else {
            return false;
        };
        let fixed = |value: Fixed64| {
            (value >= Fixed64::from_fixed(Fixed::MIN) && value <= Fixed64::from_fixed(Fixed::MAX))
                .then(|| value.to_fixed())
        };
        let (Some(x), Some(y), Some(w), Some(h)) = (fixed(x), fixed(y), fixed(w), fixed(h)) else {
            return false;
        };
        let area = Rect::new(x, y, w, h);
        if !self.viewport.rect_to_physical(area).is_aligned() {
            return false;
        }
        let alpha = super::path::solid_alpha(opacity, color.a)
            .map01(255)
            .to_int() as u8;
        self.fill_rect_inner(&area, clip, &Color { a: 255, ..*color }, Fixed::ZERO, alpha);
        true
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/line.rs"]
mod tests;
