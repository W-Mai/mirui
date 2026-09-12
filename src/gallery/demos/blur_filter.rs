#![allow(clippy::needless_update)]

extern crate alloc;

use alloc::vec::Vec;

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::resolver::SliceResolver;
use crate::render::scene::{Paint, ResourceRef, Scene, SceneOp};
use crate::types::Transform;
use crate::ui::widgets::Text;

#[derive(Default)]
pub struct BlurFilter;

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

fn shape_set(renderer: &mut dyn Renderer, clip: &Rect, offset: i32) {
    for (x, y, w, h, color) in [
        (28, 58, 118, 58, Color::rgb(255, 80, 110)),
        (82, 104, 92, 60, Color::rgb(90, 210, 255)),
        (130, 44, 68, 100, Color::rgb(255, 200, 80)),
    ] {
        renderer.draw(
            &DrawCommand::Fill {
                area: Rect::new(
                    Fixed::from_int(offset + x),
                    Fixed::from_int(y),
                    Fixed::from_int(w),
                    Fixed::from_int(h),
                ),
                transform: Transform::IDENTITY,
                quad: None,
                color,
                radius: Fixed::from_int(14),
                opa: 220,
            },
            clip,
        );
    }
    let circle = circle_path(
        Fixed::from_int(offset + 112),
        Fixed::from_int(128),
        Fixed::from_int(44),
    );
    let paint = Paint::Color(Color::rgb(145, 255, 120).into());
    renderer.draw(
        &DrawCommand::FillPath {
            path: &circle,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 230,
            fill_rule: FillRule::EvenOdd,
        },
        clip,
    );
}

fn shape_set_ops(offset: i32) -> [SceneOp; 4] {
    let paint_green = Paint::Color(Color::rgb(145, 255, 120).into());
    let of = Fixed::from_int(offset);
    let circle = circle_path(
        Fixed::from_int(offset + 112),
        Fixed::from_int(128),
        Fixed::from_int(44),
    );
    [
        SceneOp::FillRect {
            area: Rect::new(
                of + Fixed::from_int(28),
                Fixed::from_int(58),
                Fixed::from_int(118),
                Fixed::from_int(58),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(255, 80, 110),
            radius: Fixed::from_int(14),
            opa: 220,
        },
        SceneOp::FillRect {
            area: Rect::new(
                of + Fixed::from_int(82),
                Fixed::from_int(104),
                Fixed::from_int(92),
                Fixed::from_int(60),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(90, 210, 255),
            radius: Fixed::from_int(14),
            opa: 220,
        },
        SceneOp::FillRect {
            area: Rect::new(
                of + Fixed::from_int(130),
                Fixed::from_int(44),
                Fixed::from_int(68),
                Fixed::from_int(100),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(255, 200, 80),
            radius: Fixed::from_int(14),
            opa: 220,
        },
        SceneOp::FillPath {
            path: circle,
            transform: Transform::IDENTITY,
            paint: paint_green,
            opa: 230,
            fill_rule: FillRule::EvenOdd,
        },
    ]
}

fn blur_filter_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(480),
                Fixed::from_int(240),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(18, 20, 28),
            radius: Fixed::ZERO,
            opa: 255,
        },
        ctx.clip,
    );

    let mut ops: Vec<SceneOp> = Vec::new();
    ops.push(SceneOp::GroupBegin {
        transform: None,
        projective: None,
        opacity: Some(255),
        clip: None,
        mask: None,
        filter: Some(ResourceRef::Token("blur:3:3".into())),
        disjoint_hint: true,
    });
    ops.extend_from_slice(&shape_set_ops(0));
    ops.push(SceneOp::GroupEnd);

    let _ = Scene { ops }.replay(renderer, ctx.clip, &SliceResolver::new(&[], &[]));

    shape_set(renderer, ctx.clip, 242);
}

pub fn blur_filter_view() -> View {
    View::new("BlurFilter", 60, blur_filter_render).with_filter::<BlurFilter>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::FlexEnd,
            padding: Padding::all(10)
        ) {
            BlurFilter (position: Position::Absolute, left: 0, top: 0, width: 480, height: 240)
            Text ("blur:3:3", width: 120, height: 22, text_color: ColorToken::OnSurface)
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(blur_filter_view());
    app.compose(parent, build_widgets);
}
