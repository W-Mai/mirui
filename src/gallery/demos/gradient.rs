#![allow(clippy::needless_update)]

use alloc::borrow::Cow;

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
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

#[derive(Default)]
pub struct Gradient;

static LINEAR_PANEL: Path = path!(
    M 76 70
    H 180
    Q 208 70 208 98
    V 222
    Q 208 250 180 250
    H 76
    Q 48 250 48 222
    V 98
    Q 48 70 76 70
    Z
);

static RADIAL_DISC: Path = path!(
    M 200 100
    C 200 155.228 155.228 200 100 200
    C 44.772 200 0 155.228 0 100
    C 0 44.772 44.772 0 100 0
    C 155.228 0 200 44.772 200 100
    Z
);

static LINEAR_STOPS: [GradientStop; 3] = [
    GradientStop {
        offset: mirx::types::Fixed::ZERO,
        color: mirx::types::Color::rgb(50, 120, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::from_ratio(55, 100),
        color: mirx::types::Color::rgb(125, 90, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::ONE,
        color: mirx::types::Color::rgb(255, 70, 90),
    },
];

static RADIAL_STOPS: [GradientStop; 3] = [
    GradientStop {
        offset: mirx::types::Fixed::ZERO,
        color: mirx::types::Color::rgb(255, 255, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::from_ratio(42, 100),
        color: mirx::types::Color::rgb(90, 190, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::ONE,
        color: mirx::types::Color::rgb(20, 70, 190),
    },
];

fn gradient_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
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
            transform: ctx.transform.compose(&Transform::translate(rect.x, rect.y)),
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
            transform: ctx.transform.compose(
                &Transform::translate(rect.x + Fixed::from_int(254), rect.y + Fixed::from_int(74))
                    .compose(&Transform::scale(
                        Fixed::from_ratio(86, 100),
                        Fixed::from_ratio(86, 100),
                    )),
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_geometry_and_stops_stay_in_static_storage() {
        assert!(LINEAR_PANEL.is_borrowed());
        assert!(RADIAL_DISC.is_borrowed());
        assert_eq!(LINEAR_STOPS.len(), 3);
        assert_eq!(RADIAL_STOPS.len(), 3);
    }
}
