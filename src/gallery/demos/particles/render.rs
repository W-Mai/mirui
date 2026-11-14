use super::state::{DOT_RADIUS, STAGE, Swarm};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::ui::Theme;

/// The stage the swarm is painted on.
#[crate::component]
pub(super) struct Field {
    pub(super) swarm: model!(Swarm),
}

const AMBER: Color = Color::rgb(242, 184, 64);
const SLATE: Color = Color::rgb(120, 150, 210);
const MINT: Color = Color::rgb(146, 226, 214);
const CORAL: Color = Color::rgb(238, 122, 112);
const TEAL: Color = Color::rgb(68, 204, 188);

/// A ring widens from the centre while it fades: colour, strongest alpha,
/// widest radius, and the frames one pulse takes.
const RINGS: [(Color, u8, i32, u32); 3] = [
    (TEAL, 110, 72, 190),
    (AMBER, 90, 92, 250),
    (SLATE, 100, 112, 300),
];

/// A bar slides back and forth along a wall: colour, which wall (`true` is
/// the top or bottom), distance from it, length, and the frames of one sweep.
const BARS: [(Color, bool, i32, i32, u32); 3] = [
    (AMBER, true, 12, 44, 240),
    (MINT, true, STAGE.1 - 12, 36, 320),
    (CORAL, false, 12, 50, 280),
];

const BAR_THICKNESS: i32 = 6;

/// Position along a sweep of `span` units that takes `frames` each way.
fn sweep(frame: u32, frames: u32, span: i32) -> i32 {
    let at = frame % (2 * frames);
    let along = if at < frames { at } else { 2 * frames - at };
    (along as i32 * span) / frames as i32
}

//~focus-start
#[crate::view(component = Field, name = "ParticleField", priority = 60)]
fn field_render(swarm: model!(component.swarm, watch(frame)), theme: res!(Theme), paint: paint!()) {
    let canvas = fit_logical_canvas(paint.rect(), paint.transform(), STAGE.0, STAGE.1);
    let colors = [
        theme.resolve(ColorToken::Primary),
        AMBER,
        theme.resolve(ColorToken::Error),
        SLATE,
        theme.resolve(ColorToken::OnSurface),
        MINT,
    ];
    let mut stage = PlayPainter::new(paint, canvas);
    stage.fill(
        Rect::new(0, 0, STAGE.0, STAGE.1),
        theme.resolve(ColorToken::SurfaceVariant),
        Fixed::from_int(18),
    );

    let centre = Point::new(STAGE.0 / 2, STAGE.1 / 2);
    let full_turn = Fixed::from_int(360);
    for (i, (color, strongest, widest, frames)) in RINGS.into_iter().enumerate() {
        // Each ring starts at a different point of its pulse.
        let pulse = (swarm.frame + i as u32 * frames / 3) % frames;
        let progress = pulse as i32 * 100 / frames as i32;
        let radius = 8 + (widest - 8) * progress / 100;
        let alpha = (i32::from(strongest) * (100 - progress) / 100) as u8;
        stage.arc(
            centre,
            Fixed::from_int(radius),
            Fixed::ZERO,
            full_turn,
            color.scale_alpha(alpha),
            Fixed::from_int(2),
        );
    }

    for (color, along_x, lane, length, frames) in BARS {
        let span = if along_x { STAGE.0 } else { STAGE.1 } - length - 24;
        let start = 12 + sweep(swarm.frame, frames, span);
        let area = if along_x {
            Rect::new(start, lane - BAR_THICKNESS / 2, length, BAR_THICKNESS)
        } else {
            Rect::new(lane - BAR_THICKNESS / 2, start, BAR_THICKNESS, length)
        };
        stage.fill(
            area,
            color.scale_alpha(190),
            Fixed::from_int(BAR_THICKNESS / 2),
        );
    }

    for (at, color) in swarm.positions().into_iter().zip(colors) {
        stage.disc(
            at.into(),
            Fixed::from_int(2 * DOT_RADIUS),
            color.scale_alpha(46),
        );
        stage.disc(at.into(), Fixed::from_int(DOT_RADIUS), color);
    }
}
//~focus-end

pub use field_render::view as field_view;
