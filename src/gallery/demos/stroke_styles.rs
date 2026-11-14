#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

/// One row of samples; `kind` picks caps, joins, dashes or widths.
#[crate::component]
#[derive(Default)]
pub struct StrokeStyles {
    pub kind: u8,
}

static LINE: Path = path!(M 0 0 L 100 0);
static ELBOW: Path = path!(M 0 30 L 30 0 L 60 30);
const LOGICAL_WIDTH: i32 = 480;
const LOGICAL_HEIGHT: i32 = 48;
const CENTRES: [i32; 3] = [80, 240, 400];

#[allow(clippy::too_many_arguments)]
fn stroke(
    paint_ctx: &mut PaintCtx<'_>,
    path: &Path,
    transform: Transform,
    paint: &Paint,
    width: Fixed,
    cap: LineCap,
    join: LineJoin,
    dash: &[Fixed],
) {
    paint_ctx.draw(&DrawCommand::StrokePath {
        path,
        transform,
        paint,
        width,
        opa: 255,
        line_cap: cap,
        line_join: join,
        miter_limit: Fixed::from_int(4),
        dash,
    });
}

#[crate::view(component = StrokeStyles, name = "StrokeStyles", priority = 60)]
fn stroke_styles_render(row: component!(), theme: res!(Theme), paint_ctx: paint!()) {
    let caps = Paint::Color(theme.resolve(ColorToken::Primary).into());
    let joins = Paint::Color(theme.resolve(ColorToken::Error).into());
    let dashes = Paint::Color(theme.resolve(ColorToken::Success).into());
    let widths = Paint::Color(theme.resolve(ColorToken::Tertiary).into());
    let empty: [Fixed; 0] = [];
    let canvas = crate::gallery::fit_logical_canvas(
        paint_ctx.rect(),
        paint_ctx.transform(),
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
    );
    let at = |x: i32, y: i32| {
        canvas.compose(&Transform::translate(
            Fixed::from_int(x),
            Fixed::from_int(y),
        ))
    };

    match row.kind {
        0 => {
            for (centre, cap) in
                CENTRES
                    .into_iter()
                    .zip([LineCap::Butt, LineCap::Round, LineCap::Square])
            {
                stroke(
                    paint_ctx,
                    &LINE,
                    at(centre - 40, 24)
                        .compose(&Transform::scale(Fixed::from_ratio(8, 10), Fixed::ONE)),
                    &caps,
                    Fixed::from_int(12),
                    cap,
                    LineJoin::Miter,
                    &empty,
                );
            }
        }
        1 => {
            for (centre, join) in
                CENTRES
                    .into_iter()
                    .zip([LineJoin::Miter, LineJoin::Round, LineJoin::Bevel])
            {
                stroke(
                    paint_ctx,
                    &ELBOW,
                    at(centre - 30, 14),
                    &joins,
                    Fixed::from_int(11),
                    LineCap::Butt,
                    join,
                    &empty,
                );
            }
        }
        2 => {
            let dash_a = [Fixed::from_int(10), Fixed::from_int(5)];
            let dash_b = [Fixed::from_int(5), Fixed::from_int(10), Fixed::from_int(15)];
            let dash_c = [
                Fixed::from_int(20),
                Fixed::from_int(5),
                Fixed::from_int(5),
                Fixed::from_int(5),
            ];
            for (centre, dash) in CENTRES
                .into_iter()
                .zip([&dash_a[..], &dash_b[..], &dash_c[..]])
            {
                stroke(
                    paint_ctx,
                    &LINE,
                    at(centre - 50, 24),
                    &dashes,
                    Fixed::from_int(6),
                    LineCap::Round,
                    LineJoin::Round,
                    dash,
                );
            }
        }
        _ => {
            for (centre, width) in
                CENTRES
                    .into_iter()
                    .zip([Fixed::ONE, Fixed::from_int(3), Fixed::from_int(8)])
            {
                stroke(
                    paint_ctx,
                    &LINE,
                    at(centre - 50, 24),
                    &widths,
                    width,
                    LineCap::Round,
                    LineJoin::Round,
                    &empty,
                );
            }
        }
    }
}

pub use stroke_styles_render::view as stroke_styles_view;

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 6,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "Stroke styles",
                height: 28,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "Caps, joins, dashes and widths",
                height: 18,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Column (
                grow: 1.0,
                width: Dimension::percent(100),
                padding: Padding::all(14),
                row_gap: 2,
                justify: JustifyContent::SpaceBetween,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            ) {
                Text (
                    "Line caps: butt, round, square",
                    height: 18,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                StrokeStyles (kind: 0, width: Dimension::percent(100), height: 48)
                Text (
                    "Line joins: miter, round, bevel",
                    height: 18,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                StrokeStyles (kind: 1, width: Dimension::percent(100), height: 48)
                Text (
                    "Dashes: three patterns",
                    height: 18,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                StrokeStyles (kind: 2, width: Dimension::percent(100), height: 48)
                Text (
                    "Widths: 1, 3 and 8",
                    height: 18,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                StrokeStyles (kind: 3, width: Dimension::percent(100), height: 48)
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install_sample(app);
    app.with_widget(stroke_styles_view());
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 440);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_geometry_stays_in_static_storage() {
        assert!(LINE.is_borrowed());
        assert!(ELBOW.is_borrowed());
    }

    #[test]
    fn logical_canvas_scales_and_centers_inside_phone_bounds() {
        let rect = Rect {
            x: Fixed::from_int(10),
            y: Fixed::from_int(20),
            w: Fixed::from_int(320),
            h: Fixed::from_int(568),
        };
        let transform = crate::gallery::fit_logical_canvas(
            rect,
            Transform::IDENTITY,
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
        );
        assert_eq!(transform.m00, Fixed::from_ratio(2, 3));
        assert_eq!(transform.m11, Fixed::from_ratio(2, 3));
        let top_left = transform.apply_point(Point::ZERO);
        let bottom_right = transform.apply_point(Point::new(
            Fixed::from_int(LOGICAL_WIDTH),
            Fixed::from_int(LOGICAL_HEIGHT),
        ));
        assert!(top_left.x >= rect.x && top_left.y >= rect.y);
        assert!(bottom_right.x <= rect.x + rect.w);
        assert!(bottom_right.y <= rect.y + rect.h);
        assert!(
            ((top_left.x - rect.x) - (rect.x + rect.w - bottom_right.x)).abs()
                <= Fixed::from_ratio(1, 256),
        );
        assert!(
            ((top_left.y - rect.y) - (rect.y + rect.h - bottom_right.y)).abs()
                <= Fixed::from_ratio(1, 256),
        );
    }
}
