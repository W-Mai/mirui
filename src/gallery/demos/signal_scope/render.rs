use core::cell::RefCell;

use super::state::{ScopeModel, TriggerEdge};
use crate::gallery::demos::instruments::InstrumentPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::view::{View, ViewCtx};

pub(super) const BG: ColorToken = ColorToken::Surface;
pub(super) const ICE: Color = Color::rgb(222, 231, 237);
const GRID: Color = Color::rgb(46, 70, 76);
pub(super) const CHANNEL_A: Color = Color::rgb(91, 255, 220);
pub(super) const CHANNEL_B: Color = Color::rgb(255, 196, 68);
const TRIGGER: Color = Color::rgb(166, 124, 255);
const MAX_WAVE_SAMPLES: usize = 480;
const PERSISTENCE_FRAMES: u8 = 7;
const CONTROL_ARROW_CELLS: [(i32, i32); 4] = [(323, 36), (497, 35), (634, 35), (751, 35)];

static CENTER_DASHES: Path = path!(
    M 0 0 L 6 0 M 9 0 L 15 0 M 18 0 L 24 0 M 27 0 L 33 0 M 36 0 L 42 0
    M 45 0 L 51 0 M 54 0 L 60 0 M 63 0 L 69 0 M 72 0 L 78 0 M 81 0 L 87 0
    M 90 0 L 96 0 M 99 0 L 100 0
);
static TRIGGER_POINTER: Path = path!(M 0 0 L 12 0 L 6 7 Z);
static CHANNEL_POINTER: Path = path!(M 0 0 L 9 5 L 0 10 Z);
static CONTROL_UP: Path = path!(M 0 6 L 5 0 L 10 6 Z);
static CONTROL_DOWN: Path = path!(M 0 0 L 5 6 L 10 0 Z);
static TRIGGER_ARROW: Path = path!(M 0 0 L 8 4 L 0 8 Z);

#[crate::component]
#[derive(Default)]
pub(super) struct ScopeCanvas;

pub(super) struct ScopeWaveScratch {
    paths: RefCell<[Path; 2]>,
}

impl Default for ScopeWaveScratch {
    fn default() -> Self {
        Self {
            paths: RefCell::new([
                Path::try_with_capacity(MAX_WAVE_SAMPLES + 1).expect("scope wave path"),
                Path::try_with_capacity(MAX_WAVE_SAMPLES + 1).expect("scope wave path"),
            ]),
        }
    }
}

fn scope_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<ScopeCanvas>(entity) {
        return;
    }
    let Some(model) = world.resource::<ScopeModel>() else {
        return;
    };
    let Some(scratch) = world.resource::<ScopeWaveScratch>() else {
        return;
    };
    let state = model.snapshot();
    let mut painter = InstrumentPainter::new(renderer, *ctx.clip, ctx.transform);
    let scale = rect.w / Fixed::from_int(800);
    let plot = Rect::new(
        rect.x + rect.w * Fixed::from_ratio(6, 100),
        rect.y + rect.h * Fixed::from_ratio(9, 100),
        rect.w * Fixed::from_ratio(91, 100),
        rect.h * Fixed::from_ratio(64, 100),
    );
    for column in 0..=12 {
        let x = plot.x + plot.w * Fixed::from_ratio(column, 12);
        painter.line(
            Point::new(x, plot.y),
            Point::new(x, plot.y + plot.h),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            if column == 6 { 125 } else { 75 },
        );
    }
    for row in 0..=7 {
        let y = plot.y + plot.h * Fixed::from_ratio(row, 7);
        painter.line(
            Point::new(plot.x, y),
            Point::new(plot.x + plot.w, y),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            75,
        );
    }

    for channel in [0, 1] {
        let center = if channel == 0 {
            plot.y + plot.h * Fixed::from_ratio(31, 100)
        } else {
            plot.y + plot.h * Fixed::from_ratio(69, 100)
        };
        let color = if channel == 0 { CHANNEL_A } else { CHANNEL_B };
        painter.stroke_path(
            &CENTER_DASHES,
            Transform::translate(plot.x, center)
                .compose(&Transform::scale(plot.w / Fixed::from_int(100), Fixed::ONE)),
            color,
            scale.max(Fixed::from_ratio(1, 2)),
            105,
            &[],
        );
        painter.fill_path(
            &CHANNEL_POINTER,
            Transform::translate(
                plot.x - Fixed::from_int(10) * scale,
                center - Fixed::from_int(5) * scale,
            )
            .compose(&Transform::scale(scale, scale)),
            color,
            245,
        );
    }

    let tick = Fixed::from_int(4) * scale;
    for column in 0..=60 {
        let x = plot.x + plot.w * Fixed::from_ratio(column, 60);
        painter.line(
            Point::new(x, plot.y),
            Point::new(x, plot.y + tick),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            145,
        );
        painter.line(
            Point::new(x, plot.y + plot.h - tick),
            Point::new(x, plot.y + plot.h),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            145,
        );
    }
    for row in 0..=35 {
        let y = plot.y + plot.h * Fixed::from_ratio(row, 35);
        painter.line(
            Point::new(plot.x, y),
            Point::new(plot.x + tick, y),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            145,
        );
        painter.line(
            Point::new(plot.x + plot.w - tick, y),
            Point::new(plot.x + plot.w, y),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            145,
        );
    }

    let samples = plot.w.to_int().clamp(128, MAX_WAVE_SAMPLES as i32) as usize;
    let mut paths = scratch.paths.borrow_mut();
    for channel in 0..=u8::from(state.channel_b) {
        let center = if channel == 0 {
            plot.y + plot.h * Fixed::from_ratio(31, 100)
        } else {
            plot.y + plot.h * Fixed::from_ratio(69, 100)
        };
        let amplitude = plot.h * Fixed::from_ratio(if channel == 0 { 18 } else { 19 }, 100);
        let path = &mut paths[usize::from(channel)];
        let color = if channel == 0 { CHANNEL_A } else { CHANNEL_B };
        for history in (0..=PERSISTENCE_FRAMES).rev() {
            path.clear();
            if channel == 0 {
                let sweep_jitter = state.sweep_jitter(channel, history) * scale;
                for index in 0..=samples {
                    let unit_x = Fixed::from_ratio(index as i32, samples as i32);
                    let point = Point::new(
                        plot.x + plot.w * unit_x + sweep_jitter,
                        center - state.trace_sample(channel, unit_x, history) * amplitude,
                    );
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            } else {
                let bits_across = 40 + i32::from(state.time_scale) * 6;
                let digital_samples = if history <= 2 {
                    samples
                } else {
                    (samples / 2).max(96)
                };
                for index in 0..=digital_samples {
                    let unit_x = Fixed::from_ratio(index as i32, digital_samples as i32);
                    let relative_bits =
                        (unit_x - Fixed::from_ratio(1, 2)) * Fixed::from_int(bits_across);
                    let point = Point::new(
                        plot.x + plot.w * unit_x,
                        center - state.uart_wave_sample(relative_bits, history) * amplitude,
                    );
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            }
            if history == 0 {
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    Fixed::from_int(9) * scale,
                    7,
                    &[],
                );
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    Fixed::from_int(5) * scale,
                    18,
                    &[],
                );
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    Fixed::from_ratio(5, 2) * scale,
                    48,
                    &[],
                );
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    scale.max(Fixed::from_ratio(3, 4)),
                    245,
                    &[],
                );
            } else {
                let opacity = match history {
                    1 => 96,
                    2 => 68,
                    3 => 47,
                    4 => 32,
                    5 => 21,
                    6 => 14,
                    _ => 9,
                };
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    Fixed::from_int(4) * scale,
                    opacity / 3,
                    &[],
                );
                painter.stroke_path(
                    path,
                    Transform::default(),
                    color,
                    scale.max(Fixed::from_ratio(1, 2)),
                    opacity,
                    &[],
                );
            }
        }
    }

    let trigger_x = plot.x + plot.w / Fixed::from_int(2);
    let trigger_dash = Fixed::from_int(5) * scale;
    let trigger_gap = Fixed::from_int(5) * scale;
    let trigger_opacity = if state.running { 190 } else { 100 };
    let mut trigger_y = plot.y;
    while trigger_y < plot.y + plot.h {
        let end = (trigger_y + trigger_dash).min(plot.y + plot.h);
        painter.line(
            Point::new(trigger_x, trigger_y),
            Point::new(trigger_x, end),
            TRIGGER,
            scale.max(Fixed::ONE),
            trigger_opacity,
        );
        trigger_y += trigger_dash + trigger_gap;
    }
    painter.fill_path(
        &TRIGGER_POINTER,
        Transform::translate(
            trigger_x - Fixed::from_int(6) * scale,
            plot.y - Fixed::from_int(7) * scale,
        )
        .compose(&Transform::scale(scale, scale)),
        TRIGGER,
        if state.running { 220 } else { 100 },
    );
    let trigger_level_y = plot.y + plot.h * Fixed::from_ratio(31, 100)
        - state.trigger_level * plot.h * Fixed::from_ratio(18, 100);
    painter.dot(
        Point::new(trigger_x, trigger_level_y),
        Fixed::from_int(3) * scale,
        TRIGGER,
        if state.running { 235 } else { 120 },
    );

    if state.running {
        let active = Rect::new(
            rect.x + rect.w * Fixed::from_ratio(20, 800),
            rect.y + rect.h * Fixed::from_ratio(387, 480),
            rect.w * Fixed::from_ratio(78, 800),
            rect.h * Fixed::from_ratio(50, 480),
        );
        for (width, opacity) in [(Fixed::from_int(3) * scale, 42), (Fixed::ONE * scale, 230)] {
            painter.border(
                active,
                CHANNEL_A,
                width,
                Fixed::from_int(7) * scale,
                opacity,
            );
        }
    }

    let stop_size = Fixed::from_int(16) * scale;
    painter.fill(
        Rect::new(
            rect.x + rect.w * Fixed::from_ratio(139, 800) - stop_size / Fixed::from_int(2),
            rect.y + rect.h * Fixed::from_ratio(411, 480) - stop_size / Fixed::from_int(2),
            stop_size,
            stop_size,
        ),
        ICE,
        Fixed::from_int(2) * scale,
        220,
    );

    for (x, color, visible) in [
        (Fixed::from_ratio(278, 1000), CHANNEL_A, true),
        (Fixed::from_ratio(496, 1000), CHANNEL_B, state.channel_b),
    ] {
        painter.fill(
            Rect::new(
                rect.x + rect.w * x - Fixed::from_int(10) * scale,
                rect.y + rect.h * Fixed::from_ratio(900, 1000),
                Fixed::from_int(20) * scale,
                Fixed::from_int(5) * scale,
            ),
            color,
            Fixed::from_int(3) * scale,
            if visible { 245 } else { 55 },
        );
    }

    let icon_x = rect.x + rect.w * Fixed::from_ratio(716, 800);
    let icon_y = rect.y + rect.h * Fixed::from_ratio(426, 480);
    let icon_step = Fixed::from_int(8) * scale;
    let icon_rise = if state.trigger_edge == TriggerEdge::Rising {
        icon_step
    } else {
        -icon_step
    };
    painter.line(
        Point::new(icon_x - icon_step, icon_y),
        Point::new(icon_x, icon_y),
        TRIGGER,
        Fixed::from_int(2) * scale,
        220,
    );
    let scale_y = rect.h / Fixed::from_int(480);
    for (cell_x, cell_width) in CONTROL_ARROW_CELLS {
        let x = rect.x
            + rect.w * Fixed::from_ratio(cell_x, 800)
            + (rect.w * Fixed::from_ratio(cell_width, 800) - Fixed::from_int(10) * scale)
                / Fixed::from_int(2);
        for (y, path) in [(394, &CONTROL_UP), (423, &CONTROL_DOWN)] {
            painter.fill_path(
                path,
                Transform::translate(x, rect.y + rect.h * Fixed::from_ratio(y, 480))
                    .compose(&Transform::scale(scale, scale_y)),
                ICE,
                190,
            );
        }
    }
    painter.line(
        Point::new(icon_x, icon_y),
        Point::new(icon_x, icon_y - icon_rise),
        TRIGGER,
        Fixed::from_int(2) * scale,
        220,
    );
    painter.line(
        Point::new(icon_x, icon_y - icon_rise),
        Point::new(icon_x + icon_step, icon_y - icon_rise),
        TRIGGER,
        Fixed::from_int(2) * scale,
        220,
    );
    painter.fill_path(
        &TRIGGER_ARROW,
        Transform::translate(
            icon_x + icon_step - Fixed::from_int(1) * scale,
            icon_y - icon_rise - Fixed::from_int(4) * scale,
        )
        .compose(&Transform::scale(scale, scale)),
        TRIGGER,
        220,
    );
    ctx.record(painter.finish());
}

pub(super) fn scope_view() -> View {
    View::new("ScopeCanvas", 60, scope_render).with_filter::<ScopeCanvas>()
}
