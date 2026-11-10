use super::geometry::{Geometry, light_point};
use super::state::{GlassModel, Scene, TRAVEL, smooth};
use super::textures;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::scene::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
use crate::types::Transform;
use crate::ui::view::PaintCtx;
use alloc::borrow::Cow;

#[crate::component]
pub(super) struct GlassToggle {
    pub model: model!(GlassModel),
}

static CIRCLE: Path = Path::from_static(&super::geometry::rounded_rect(1_000, 1_000, 1_000));
static TRACK: Path = Path::from_static(&super::geometry::rounded_rect(216_000, 112_000, 72_000));
static KNOB: Path = Path::from_static(&super::geometry::rounded_rect(99_000, 99_000, 58_000));
static TRACK_RIM: Path = Path::from_static(&super::geometry::ring(216_000, 112_000, 72_000, 1_000));
static KNOB_RIM: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 1_000));
static LED_22: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 22_000));
static LED_12: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 12_000));
static LED_6: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 6_000));
static LED_3: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 3_000));
static LED_1_5: Path = Path::from_static(&super::geometry::ring(99_000, 99_000, 58_000, 1_500));

const fn stop(offset: i32, r: u8, g: u8, b: u8, a: u8) -> GradientStop {
    GradientStop {
        offset: mirx::types::Fixed::from_ratio(offset, 100),
        color: mirx::types::Color { r, g, b, a },
    }
}
static TRACK_DARK: [GradientStop; 3] = [
    stop(0, 3, 3, 4, 255),
    stop(55, 7, 7, 9, 255),
    stop(100, 14, 14, 16, 255),
];
static TRACK_LIGHT: [GradientStop; 3] = [
    stop(0, 203, 203, 201, 255),
    stop(25, 225, 225, 222, 255),
    stop(100, 244, 243, 239, 255),
];
static LENS_DARK: [GradientStop; 3] = [
    stop(0, 61, 62, 67, 232),
    stop(42, 37, 38, 43, 240),
    stop(100, 28, 30, 38, 242),
];
static LENS_LIGHT: [GradientStop; 3] = [
    stop(0, 244, 243, 237, 255),
    stop(55, 247, 245, 239, 255),
    stop(100, 251, 250, 247, 255),
];
static SHADOW: [GradientStop; 3] = [
    stop(0, 0, 0, 0, 180),
    stop(50, 0, 0, 0, 95),
    stop(100, 0, 0, 0, 0),
];
static BLUE_GLOW: [GradientStop; 4] = [
    stop(0, 146, 161, 255, 195),
    stop(24, 106, 126, 255, 123),
    stop(60, 89, 112, 255, 40),
    stop(100, 88, 108, 255, 0),
];
static AMBER_GLOW: [GradientStop; 4] = [
    stop(0, 255, 196, 87, 220),
    stop(24, 255, 174, 56, 150),
    stop(60, 255, 176, 65, 40),
    stop(100, 255, 183, 80, 0),
];
static BLUE_LED: [GradientStop; 4] = [
    stop(0, 226, 236, 255, 255),
    stop(30, 180, 195, 255, 255),
    stop(74, 103, 127, 255, 180),
    stop(100, 103, 127, 255, 0),
];
static AMBER_LED: [GradientStop; 4] = [
    stop(0, 255, 255, 220, 255),
    stop(30, 255, 218, 105, 255),
    stop(74, 255, 172, 39, 160),
    stop(100, 255, 172, 39, 0),
];
static WHITE_LED: [GradientStop; 3] = [
    stop(0, 255, 255, 255, 255),
    stop(66, 255, 255, 255, 235),
    stop(100, 255, 255, 255, 0),
];
static RIM: [GradientStop; 4] = [
    stop(0, 255, 255, 255, 150),
    stop(20, 234, 240, 255, 47),
    stop(70, 155, 165, 195, 35),
    stop(100, 255, 255, 255, 170),
];
static SHEEN: [GradientStop; 3] = [
    stop(0, 255, 255, 255, 26),
    stop(30, 255, 255, 255, 2),
    stop(100, 255, 255, 255, 0),
];

fn fp(x: Fixed, y: Fixed) -> mirx::types::Point {
    Point::new(x, y).into()
}
fn linear(stops: &'static [GradientStop]) -> Paint {
    Paint::LinearGradient(LinearGradient {
        start: fp(Fixed::ZERO, Fixed::ZERO),
        end: fp(Fixed::ZERO, Fixed::ONE),
        stops: Cow::Borrowed(stops),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}
fn radial(stops: &'static [GradientStop], center: Point, radius: Fixed) -> Paint {
    Paint::RadialGradient(RadialGradient {
        center: center.into(),
        focal: center.into(),
        radius: radius.into(),
        focal_radius: Fixed::ZERO.into(),
        stops: Cow::Borrowed(stops),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}
fn alpha(value: Fixed) -> u8 {
    (value.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_int(255)).to_int() as u8
}
fn local(x: Fixed, y: Fixed, sx: Fixed, sy: Fixed) -> Transform {
    Transform::translate(x, y).compose(&Transform::scale(sx, sy))
}
struct Painter<'p, 'f> {
    paint: &'p mut PaintCtx<'f>,
    transform: Transform,
}
impl Painter<'_, '_> {
    fn image(&mut self, texture: &crate::render::texture::Texture<'_>, rect: Rect, opacity: u8) {
        if opacity == 0 {
            return;
        }
        let aligned = self.transform.m01 == Fixed::ZERO
            && self.transform.m10 == Fixed::ZERO
            && self.transform.m00 > Fixed::ZERO
            && self.transform.m11 > Fixed::ZERO;
        let area = if aligned {
            self.transform.apply_rect_bbox(rect)
        } else {
            rect
        };
        self.paint.draw(&DrawCommand::Blit {
            pos: Point::new(area.x, area.y),
            size: Point::new(area.w, area.h),
            transform: if aligned {
                Transform::IDENTITY
            } else {
                self.transform
            },
            quad: (!aligned).then(|| self.transform.apply_rect(rect)),
            texture,
            opa: opacity,
            radius: Fixed::ZERO,
            composite: crate::render::command::CompositeMode::SourceOver,
        });
    }
    fn path(&mut self, path: &Path, pose: Transform, material: &Paint, opacity: u8) {
        if opacity == 0 {
            return;
        }
        self.paint.draw(&DrawCommand::FillPath {
            path,
            transform: self.transform.compose(&pose),
            paint: material,
            opa: opacity,
            fill_rule: FillRule::EvenOdd,
        });
    }
    fn fill(&mut self, rect: Rect, color: Color, radius: Fixed, opacity: u8) {
        if opacity == 0 {
            return;
        }
        let aligned = self.transform.m01 == Fixed::ZERO
            && self.transform.m10 == Fixed::ZERO
            && self.transform.m00 > Fixed::ZERO
            && self.transform.m00 == self.transform.m11;
        self.paint.draw(&DrawCommand::Fill {
            area: if aligned {
                self.transform.apply_rect_bbox(rect)
            } else {
                rect
            },
            transform: if aligned {
                Transform::IDENTITY
            } else {
                self.transform
            },
            quad: None,
            color,
            radius: if aligned {
                radius * self.transform.m00
            } else {
                radius
            },
            opa: opacity,
        });
    }
}

fn paint_scene(paint: &mut PaintCtx<'_>, scene: &Scene) {
    let rect = paint.rect();
    let g = Geometry::new(rect);
    let p = scene.progress.clamp(Fixed::ZERO, Fixed::ONE);
    let day = smooth((p - Fixed::from_ratio(18, 100)) / Fixed::from_ratio(64, 100));
    let mut light = light_point(scene.light);
    light.x = light.x * Fixed::from_int(i32::from(scene.light_direction));
    let x = (p - Fixed::HALF) * Fixed::from_int(TRAVEL);
    let mut background = Painter {
        transform: paint.transform(),
        paint,
    };
    background.image(&textures::dark(), rect, 255);
    background.image(&textures::light(), rect, alpha(day));
    let transform = background
        .paint
        .transform()
        .compose(&local(g.center.x, g.center.y, g.scale, g.scale));
    let mut painter = Painter {
        paint: background.paint,
        transform,
    };
    let halo = Point::new(x + light.x, light.y);
    let ambient = Rect::new(
        halo.x - Fixed::from_int(210),
        halo.y - Fixed::from_int(180),
        420,
        360,
    );
    painter.image(&textures::blue(), ambient, alpha(Fixed::ONE - day));
    painter.image(&textures::amber(), ambient, alpha(day));
    painter.path(
        &CIRCLE,
        local(
            Fixed::ZERO,
            Fixed::from_int(75),
            Fixed::from_int(270),
            Fixed::from_int(100),
        ),
        &radial(&SHADOW, Point::new(Fixed::HALF, Fixed::HALF), Fixed::HALF),
        alpha(Fixed::from_ratio(25, 100) + day * Fixed::from_ratio(25, 100)),
    );
    painter.path(&TRACK, Transform::IDENTITY, &linear(&TRACK_DARK), 255);
    painter.path(
        &TRACK,
        Transform::IDENTITY,
        &linear(&TRACK_LIGHT),
        alpha(day),
    );
    painter.path(
        &TRACK_RIM,
        Transform::IDENTITY,
        &linear(&RIM),
        alpha(Fixed::from_ratio(1, 10) + day * Fixed::from_ratio(35, 100)),
    );
    painter.path(
        &CIRCLE,
        local(
            x,
            Fixed::from_int(30),
            Fixed::from_int(130),
            Fixed::from_int(132),
        ),
        &radial(&SHADOW, Point::new(Fixed::HALF, Fixed::HALF), Fixed::HALF),
        110,
    );
    let stretch = (scene.velocity.abs() * Fixed::from_ratio(65, 10)).min(Fixed::from_int(14));
    let squeeze = Fixed::ONE - scene.press * Fixed::from_ratio(35, 1000);
    let sx = (Fixed::ONE + stretch / 99) * squeeze;
    let sy = (Fixed::ONE - stretch * Fixed::from_ratio(45, 100) / 99) * squeeze;
    let knob = local(x, Fixed::ZERO, sx, sy);
    painter.path(&KNOB, knob, &linear(&LENS_DARK), 255);
    painter.path(&KNOB, knob, &linear(&LENS_LIGHT), alpha(day));
    let origin = Point::new(
        (light.x + Fixed::from_int(99)) / 198,
        (light.y + Fixed::from_int(99)) / 198,
    );
    painter.path(
        &KNOB,
        knob,
        &radial(&BLUE_GLOW, origin, Fixed::from_ratio(9, 10)),
        alpha(Fixed::ONE - day),
    );
    painter.path(
        &KNOB,
        knob,
        &radial(&AMBER_GLOW, origin, Fixed::from_ratio(9, 10)),
        alpha(day),
    );
    let pixel_color = Color::rgb(255, 247, 229).blend_with(Color::rgb(57, 48, 35), day);
    for pixel in scene.pixels {
        if pixel.alpha == 0 {
            continue;
        }
        let center = Point::new(
            x + pixel.position.x * Fixed::from_ratio(106, 100),
            pixel.position.y * Fixed::from_ratio(106, 100),
        );
        let side = pixel.size;
        let glow = side + Fixed::from_int(6);
        painter.fill(
            Rect::new(center.x - glow / 2, center.y - glow / 2, glow, glow),
            pixel_color,
            Fixed::from_int(3),
            ((u16::from(pixel.alpha) * 18) / 255) as u8,
        );
        painter.fill(
            Rect::new(center.x - side / 2, center.y - side / 2, side, side),
            pixel_color,
            Fixed::from_ratio(2, 10),
            pixel.alpha,
        );
    }
    painter.path(&KNOB, knob, &linear(&SHEEN), 255);
    painter.path(&KNOB_RIM, knob, &linear(&RIM), 255);
    let blue = radial(&BLUE_LED, origin, Fixed::from_ratio(65, 100));
    let amber = radial(&AMBER_LED, origin, Fixed::from_ratio(65, 100));
    for (ring, opacity) in [(&LED_22, 40), (&LED_12, 80), (&LED_6, 170), (&LED_3, 255)] {
        painter.path(
            ring,
            knob,
            &blue,
            ((u16::from(alpha(Fixed::ONE - day)) * opacity) / 255) as u8,
        );
        painter.path(
            ring,
            knob,
            &amber,
            ((u16::from(alpha(day)) * opacity) / 255) as u8,
        );
    }
    painter.path(
        &LED_1_5,
        knob,
        &radial(&WHITE_LED, origin, Fixed::from_ratio(52, 100)),
        245,
    );
}

#[crate::view(component=GlassToggle,name="Glass Light Toggle",priority=60,
input=(super::input::tap::callback(),super::input::drag_start::callback(),super::input::drag_move::callback(),super::input::drag_end::callback(),super::input::drag_cancel::callback(),super::input::key::callback()),
systems=[super::lead_host::system(), super::follow_host::system(), super::animate::system()])]
pub(super) fn glass_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    paint_scene(paint, &model.scene());
}
