#![allow(clippy::needless_update)]

use alloc::borrow::Cow;

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
use crate::types::Transform;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const LOGICAL_WIDTH: i32 = 480;
const LOGICAL_HEIGHT: i32 = 320;

fn canvas_transform(rect: &Rect, parent: Transform) -> Transform {
    let scale =
        (rect.w / Fixed::from_int(LOGICAL_WIDTH)).min(rect.h / Fixed::from_int(LOGICAL_HEIGHT));
    let width = Fixed::from_int(LOGICAL_WIDTH) * scale;
    let height = Fixed::from_int(LOGICAL_HEIGHT) * scale;
    let x = rect.x + (rect.w - width) / Fixed::from_int(2);
    let y = rect.y + (rect.h - height) / Fixed::from_int(2);
    parent
        .compose(&Transform::translate(x, y))
        .compose(&Transform::scale(scale, scale))
}

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
    let canvas = canvas_transform(rect, ctx.transform);
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
    View::new("Gradient", 60, gradient_render).with_filter::<Gradient>()
}

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
                "GRADIENTS",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "object-space linear + focal radial",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Gradient (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
        }
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

    #[test]
    fn logical_canvas_stays_inside_phone_bounds() {
        let rect = Rect::new(8, 72, 304, 480);
        let transform = canvas_transform(&rect, Transform::IDENTITY);
        let top_left = transform.apply_point(Point::ZERO);
        let bottom_right = transform.apply_point(Point::new(
            Fixed::from_int(LOGICAL_WIDTH),
            Fixed::from_int(LOGICAL_HEIGHT),
        ));
        assert!(top_left.x >= rect.x && top_left.y >= rect.y);
        assert!(bottom_right.x <= rect.x + rect.w);
        assert!(bottom_right.y <= rect.y + rect.h);
    }
}
