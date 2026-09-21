#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::Theme;

#[derive(Default)]
pub struct StrokeStyles;

static LINE: Path = path!(M 0 0 L 100 0);
static ELBOW: Path = path!(M 0 42 L 42 0 L 84 42);
static GUIDE: Path = path!(M 0 0 H 448 V 320 H 0 Z);
const LOGICAL_WIDTH: i32 = 480;
const LOGICAL_HEIGHT: i32 = 360;

#[allow(clippy::too_many_arguments)]
fn stroke(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    path: &Path,
    transform: Transform,
    paint: &Paint,
    width: Fixed,
    cap: LineCap,
    join: LineJoin,
    dash: &[Fixed],
) {
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path,
            transform,
            paint,
            width,
            opa: 255,
            line_cap: cap,
            line_join: join,
            miter_limit: Fixed::from_int(4),
            dash,
        },
        ctx.clip,
    );
}

fn stroke_styles_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let paint = Paint::Color(theme.resolve(ColorToken::Primary).into());
    let hot = Paint::Color(theme.resolve(ColorToken::Error).into());
    let green = Paint::Color(theme.resolve(ColorToken::Success).into());
    let violet = Paint::Color(theme.resolve(ColorToken::Tertiary).into());
    let empty: [Fixed; 0] = [];
    let canvas =
        crate::gallery::fit_logical_canvas(*rect, ctx.transform, LOGICAL_WIDTH, LOGICAL_HEIGHT);

    for (i, cap) in [LineCap::Butt, LineCap::Round, LineCap::Square]
        .into_iter()
        .enumerate()
    {
        let x = 44 + i as i32 * 144;
        stroke(
            renderer,
            ctx,
            &LINE,
            canvas.compose(
                &Transform::translate(Fixed::from_int(x), Fixed::from_int(54))
                    .compose(&Transform::scale(Fixed::from_ratio(92, 100), Fixed::ONE)),
            ),
            &paint,
            Fixed::from_int(10),
            cap,
            LineJoin::Miter,
            &empty,
        );
    }

    for (i, join) in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel]
        .into_iter()
        .enumerate()
    {
        stroke(
            renderer,
            ctx,
            &ELBOW,
            canvas.compose(&Transform::translate(
                Fixed::from_int(38 + i as i32 * 146),
                Fixed::from_int(108),
            )),
            &hot,
            Fixed::from_int(11),
            LineCap::Butt,
            join,
            &empty,
        );
    }

    let dash_a = [Fixed::from_int(10), Fixed::from_int(5)];
    let dash_b = [Fixed::from_int(5), Fixed::from_int(10), Fixed::from_int(15)];
    let dash_c = [
        Fixed::from_int(20),
        Fixed::from_int(5),
        Fixed::from_int(5),
        Fixed::from_int(5),
    ];
    for (i, dash) in [&dash_a[..], &dash_b[..], &dash_c[..]]
        .into_iter()
        .enumerate()
    {
        let x = 42 + i as i32 * 146;
        stroke(
            renderer,
            ctx,
            &LINE,
            canvas.compose(&Transform::translate(
                Fixed::from_int(x),
                Fixed::from_int(214),
            )),
            &green,
            Fixed::from_int(6),
            LineCap::Round,
            LineJoin::Round,
            dash,
        );
    }

    for (i, width) in [Fixed::ONE, Fixed::from_int(3), Fixed::from_int(8)]
        .into_iter()
        .enumerate()
    {
        let x = 46 + i as i32 * 146;
        stroke(
            renderer,
            ctx,
            &LINE,
            canvas.compose(
                &Transform::translate(Fixed::from_int(x), Fixed::from_int(304))
                    .compose(&Transform::scale(Fixed::from_ratio(98, 100), Fixed::ONE)),
            ),
            &violet,
            width,
            LineCap::Round,
            LineJoin::Round,
            &empty,
        );
    }

    let guide_paint = Paint::Color(theme.resolve(ColorToken::Outline).into());
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &GUIDE,
            transform: canvas.compose(&Transform::translate(
                Fixed::from_int(16),
                Fixed::from_int(20),
            )),
            paint: &guide_paint,
            width: Fixed::ONE,
            opa: 180,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Bevel,
            miter_limit: Fixed::from_int(4),
            dash: &empty,
        },
        ctx.clip,
    );
}

pub fn stroke_styles_view() -> View {
    View::new("StrokeStyles", 60, stroke_styles_render).with_filter::<StrokeStyles>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        StrokeStyles (grow: 1.0)
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(stroke_styles_view());
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 360);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_geometry_stays_in_static_storage() {
        assert!(LINE.is_borrowed());
        assert!(ELBOW.is_borrowed());
        assert!(GUIDE.is_borrowed());
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
