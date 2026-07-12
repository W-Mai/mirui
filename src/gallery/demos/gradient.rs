#![allow(clippy::needless_update)]

extern crate alloc;

use alloc::vec;

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
use crate::types::Transform;

fn unit_point(x: f32, y: f32) -> mirx::Point {
    Point {
        x: Fixed::from_f32(x),
        y: Fixed::from_f32(y),
    }
    .into()
}

fn unit(v: f32) -> mirx::Fixed {
    Fixed::from_f32(v).into()
}

#[derive(Default)]
pub struct Gradient;

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

fn gradient_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let left = Path::rounded_rect(
        rect.x + Fixed::from_int(48),
        rect.y + Fixed::from_int(70),
        Fixed::from_int(160),
        Fixed::from_int(180),
        Fixed::from_int(28),
    );
    let linear = Paint::LinearGradient(LinearGradient {
        start: unit_point(0.0, 0.0),
        end: unit_point(1.0, 1.0),
        stops: vec![
            GradientStop {
                offset: unit(0.0),
                color: Color::rgb(50, 120, 255).into(),
            },
            GradientStop {
                offset: unit(0.55),
                color: Color::rgb(125, 90, 255).into(),
            },
            GradientStop {
                offset: unit(1.0),
                color: Color::rgb(255, 70, 90).into(),
            },
        ],
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    });
    renderer.draw(
        &DrawCommand::FillPath {
            path: &left,
            transform: ctx.transform,
            paint: &linear,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );

    let circle = circle_path(
        rect.x + Fixed::from_int(340),
        rect.y + Fixed::from_int(160),
        Fixed::from_int(86),
    );
    let radial = Paint::RadialGradient(RadialGradient {
        center: unit_point(0.42, 0.38),
        radius: unit(0.7),
        focal: unit_point(0.35, 0.32),
        focal_radius: unit(0.0),
        stops: vec![
            GradientStop {
                offset: unit(0.0),
                color: Color::rgb(255, 255, 255).into(),
            },
            GradientStop {
                offset: unit(0.42),
                color: Color::rgb(90, 190, 255).into(),
            },
            GradientStop {
                offset: unit(1.0),
                color: Color::rgb(20, 70, 190).into(),
            },
        ],
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    });
    renderer.draw(
        &DrawCommand::FillPath {
            path: &circle,
            transform: ctx.transform,
            paint: &radial,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
}

pub fn gradient_view() -> View {
    View::new("Gradient", 60, gradient_render).with_filter::<Gradient>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        Gradient (grow: 1.0)
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(gradient_view());
    app.compose(parent, build_widgets);
}
