use super::style::{ACCENT, STAR_SLOT};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::lumen::{LumenModel, MAX_STARS};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::types::Transform;

/// A five-pointed star of radius 10, centred on the origin.
static STAR: Path = crate::path!(
    "M 0 -10 L 2.351 -3.236 L 9.511 -3.09 L 3.804 1.236 L 5.878 8.09 L 0 4 L -5.878 8.09 L -3.804 1.236 L -9.511 -3.09 L -2.351 -3.236 Z"
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StarRow {
    /// What a solve would give now, small in the side panel.
    Live,
    /// The stars of a solve, landing one by one on the result card.
    Result,
}

impl StarRow {
    /// The row's own size and the radius of its stars.
    const fn geometry(self) -> (i32, i32, i32) {
        match self {
            Self::Live => (66, 18, 7),
            Self::Result => (150, 40, 16),
        }
    }
}

#[crate::component]
pub(super) struct LumenStars {
    pub(super) model: model!(LumenModel),
    pub(super) row: StarRow,
}

fn paint_star(painter: &mut PlayPainter<'_, '_>, center: Point, radius: Fixed, color: Color) {
    let size = radius / Fixed::from_int(10);
    painter
        .transformed(
            Transform::translate(center.x, center.y).compose(&Transform::scale(size, size)),
        )
        .fill_path(&STAR, color);
}

#[crate::view(component = LumenStars, name = "LumenStars", priority = 60)]
pub(super) fn stars_render(
    component: component!(),
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    let (width, height, radius) = component.row.geometry();
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), width, height);
    let mut painter = PlayPainter::new(paint, transform);
    let spacing = width / i32::from(MAX_STARS);
    let shown = if model.solved() {
        model.earned()
    } else {
        model.live_rating()
    };
    for star in 0..i32::from(MAX_STARS) {
        let center = Point::new(spacing / 2 + star * spacing, height / 2);
        let radius = Fixed::from_int(radius);
        paint_star(&mut painter, center, radius, STAR_SLOT);
        let scale = match component.row {
            StarRow::Live if star < i32::from(shown) => Fixed::ONE,
            StarRow::Live => Fixed::ZERO,
            StarRow::Result => model.motion().star(star as usize),
        };
        if scale > Fixed::ZERO {
            paint_star(&mut painter, center, radius * scale, ACCENT);
        }
    }
}
