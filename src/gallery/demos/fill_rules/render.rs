use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::Theme;

#[crate::component]
#[derive(Default)]
pub struct FillRules;

pub(super) static STAR: Path = path!(
    M 100 18
    L 148.198 166.34
    L 22.013 74.661
    L 177.987 74.661
    L 51.802 166.34
    Z
);
pub(super) const LOGICAL_WIDTH: i32 = 400;
pub(super) const LOGICAL_HEIGHT: i32 = 240;

#[crate::view(component = FillRules, name = "FillRules", priority = 60)]
fn fill_rules_render(theme: res!(optional Theme), paint: paint!()) {
    let default_theme = Theme::default();
    let theme = theme.as_deref().unwrap_or(&default_theme);
    let fill = Paint::Color(theme.resolve(ColorToken::Secondary).into());
    let stroke = Paint::Color(theme.resolve(ColorToken::OnSurface).into());
    let canvas = crate::gallery::fit_logical_canvas(
        paint.rect(),
        paint.transform(),
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
    );

    for (transform, rule) in [
        (
            Transform::translate(Fixed::from_int(12), Fixed::from_int(16)),
            FillRule::EvenOdd,
        ),
        (
            Transform::translate(Fixed::from_int(188), Fixed::from_int(16)),
            FillRule::NonZero,
        ),
    ] {
        paint.draw(&DrawCommand::FillPath {
            path: &STAR,
            transform: canvas.compose(&transform),
            paint: &fill,
            opa: 245,
            fill_rule: rule,
        });
        paint.draw(&DrawCommand::StrokePath {
            path: &STAR,
            transform: canvas.compose(&transform),
            paint: &stroke,
            width: Fixed::from_int(2),
            opa: 235,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        });
    }
}

pub use fill_rules_render::view as fill_rules_view;
