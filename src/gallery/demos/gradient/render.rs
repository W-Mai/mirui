use alloc::borrow::Cow;

use super::scene::{
    LINEAR_PANEL, LINEAR_STOPS, LOGICAL_HEIGHT, LOGICAL_WIDTH, RADIAL_DISC, RADIAL_STOPS,
};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode};
use crate::types::Transform;

fn unit_point(x: f32, y: f32) -> mirx::types::Point {
    Point {
        x: Fixed::from_f32(x),
        y: Fixed::from_f32(y),
    }
    .into()
}

fn unit(v: f32) -> mirx::types::Fixed {
    Fixed::from_f32(v).into()
}

#[crate::component]
#[derive(Default)]
pub struct Gradient;

#[crate::view(component = Gradient, name = "Gradient", priority = 60)]
fn gradient_render(renderer: &mut dyn Renderer, rect: &Rect, ctx: &mut ViewCtx) {
    let canvas =
        crate::gallery::fit_logical_canvas(*rect, ctx.transform, LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let linear = Paint::LinearGradient(LinearGradient {
        start: unit_point(0.0, 0.0),
        end: unit_point(1.0, 1.0),
        stops: Cow::Borrowed(&LINEAR_STOPS),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    });
    ctx.draw(
        renderer,
        &DrawCommand::FillPath {
            path: &LINEAR_PANEL,
            transform: canvas,
            paint: &linear,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );

    let radial = Paint::RadialGradient(RadialGradient {
        center: unit_point(0.42, 0.38),
        radius: unit(0.7),
        focal: unit_point(0.35, 0.32),
        focal_radius: unit(0.0),
        stops: Cow::Borrowed(&RADIAL_STOPS),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    });
    ctx.draw(
        renderer,
        &DrawCommand::FillPath {
            path: &RADIAL_DISC,
            transform: canvas.compose(
                &Transform::translate(Fixed::from_int(254), Fixed::from_int(74)).compose(
                    &Transform::scale(Fixed::from_ratio(86, 100), Fixed::from_ratio(86, 100)),
                ),
            ),
            paint: &radial,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
}

pub fn gradient_view() -> View {
    gradient_render::view()
}
