#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::Paint;
use crate::types::Transform;

#[derive(Default)]
pub struct ClipPath;

fn circle_path(cx: Fixed, cy: Fixed, r: Fixed) -> Path {
    let k = r * Fixed::from_f32(0.552_284_8);
    let mut path = Path::new();
    path.move_to(Point { x: cx + r, y: cy });
    path.cubic_to(
        Point {
            x: cx + r,
            y: cy + k,
        },
        Point {
            x: cx + k,
            y: cy + r,
        },
        Point { x: cx, y: cy + r },
    );
    path.cubic_to(
        Point {
            x: cx - k,
            y: cy + r,
        },
        Point {
            x: cx - r,
            y: cy + k,
        },
        Point { x: cx - r, y: cy },
    );
    path.cubic_to(
        Point {
            x: cx - r,
            y: cy - k,
        },
        Point {
            x: cx - k,
            y: cy - r,
        },
        Point { x: cx, y: cy - r },
    );
    path.cubic_to(
        Point {
            x: cx + k,
            y: cy - r,
        },
        Point {
            x: cx + r,
            y: cy - k,
        },
        Point { x: cx + r, y: cy },
    );
    path.close();
    path
}

fn fill_rect(
    renderer: &mut dyn Renderer,
    clip: &Rect,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: Color,
) {
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect::new(
                Fixed::from_int(x),
                Fixed::from_int(y),
                Fixed::from_int(w),
                Fixed::from_int(h),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color,
            radius: Fixed::from_int(10),
            opa: 255,
        },
        clip,
    );
}

fn clip_path_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let grays = [
        Color::rgb(58, 60, 68),
        Color::rgb(72, 74, 82),
        Color::rgb(86, 88, 96),
        Color::rgb(100, 102, 110),
    ];
    let colors = [
        Color::rgb(255, 90, 110),
        Color::rgb(255, 190, 80),
        Color::rgb(80, 210, 160),
        Color::rgb(80, 145, 255),
    ];

    for (i, color) in grays.into_iter().enumerate() {
        fill_rect(renderer, ctx.clip, 30, 42 + i as i32 * 56, 260, 42, color);
    }

    let clip_path = circle_path(
        Fixed::from_int(160),
        Fixed::from_int(160),
        Fixed::from_int(104),
    );
    renderer.draw(
        &DrawCommand::PushClip {
            path: &clip_path,
            transform: Transform::IDENTITY,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
    for (i, color) in colors.into_iter().enumerate() {
        fill_rect(renderer, ctx.clip, 30, 42 + i as i32 * 56, 260, 42, color);
    }
    renderer.draw(&DrawCommand::PopClip, ctx.clip);

    let outline = Paint::Color(Color::rgb(230, 235, 245).into());
    renderer.draw(
        &DrawCommand::StrokePath {
            path: &clip_path,
            transform: Transform::IDENTITY,
            paint: &outline,
            width: Fixed::from_int(2),
            opa: 220,
            line_cap: crate::render::scene::LineCap::Round,
            line_join: crate::render::scene::LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        },
        ctx.clip,
    );
}

pub fn clip_path_view() -> View {
    View::new("ClipPath", 60, clip_path_render).with_filter::<ClipPath>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        ClipPath (grow: 1.0)
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(clip_path_view());
    app.compose(parent, build_widgets);
}
