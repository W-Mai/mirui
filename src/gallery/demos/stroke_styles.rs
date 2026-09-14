#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;

#[derive(Default)]
pub struct StrokeStyles;

fn line_path(x1: i32, y: i32, x2: i32) -> Path {
    let mut path = Path::new();
    path.move_to(Point {
        x: Fixed::from_int(x1),
        y: Fixed::from_int(y),
    });
    path.line_to(Point {
        x: Fixed::from_int(x2),
        y: Fixed::from_int(y),
    });
    path
}

fn elbow_path(x: i32, y: i32) -> Path {
    let mut path = Path::new();
    path.move_to(Point {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y + 42),
    });
    path.line_to(Point {
        x: Fixed::from_int(x + 42),
        y: Fixed::from_int(y),
    });
    path.line_to(Point {
        x: Fixed::from_int(x + 84),
        y: Fixed::from_int(y + 42),
    });
    path
}

#[allow(clippy::too_many_arguments)]
fn stroke(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    path: &Path,
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
            transform: Transform::IDENTITY,
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
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let paint = Paint::Color(Color::rgb(90, 190, 255).into());
    let hot = Paint::Color(Color::rgb(255, 125, 95).into());
    let green = Paint::Color(Color::rgb(120, 225, 150).into());
    let violet = Paint::Color(Color::rgb(180, 140, 255).into());
    let empty: [Fixed; 0] = [];

    for (i, cap) in [LineCap::Butt, LineCap::Round, LineCap::Square]
        .into_iter()
        .enumerate()
    {
        let x = 44 + i as i32 * 144;
        let path = line_path(x, 54, x + 92);
        stroke(
            renderer,
            ctx,
            &path,
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
        let path = elbow_path(38 + i as i32 * 146, 108);
        stroke(
            renderer,
            ctx,
            &path,
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
        let path = line_path(x, 214, x + 100);
        stroke(
            renderer,
            ctx,
            &path,
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
        let path = line_path(x, 304, x + 98);
        stroke(
            renderer,
            ctx,
            &path,
            &violet,
            width,
            LineCap::Round,
            LineJoin::Round,
            &empty,
        );
    }

    let guide = Path::rect(
        Fixed::from_int(16),
        Fixed::from_int(20),
        Fixed::from_int(448),
        Fixed::from_int(320),
    );
    let guide_paint = Paint::Color(Color::rgb(50, 55, 70).into());
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &guide,
            transform: Transform::IDENTITY,
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
