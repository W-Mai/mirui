//! Interactive dual-channel signal scope.

use core::cell::RefCell;

use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::DeltaTimeMs;
use crate::gallery::demos::instruments::{InstrumentPainter, register_fonts};
use crate::prelude::*;
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::render::texture::MirxTextureOptions;
use crate::types::Transform;
use crate::ui::IgnoreHitTest;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Image, ParagraphStyle, Text};

pub const VIEWPORT: (u16, u16) = (800, 480);

const HOUSING_ART: &[u8] = include_bytes!("assets/product/signal-scope-housing.mirx");

const BG: ColorToken = ColorToken::Surface;
const ICE: Color = Color::rgb(222, 231, 237);
const GRID: Color = Color::rgb(46, 70, 76);
const CHANNEL_A: Color = Color::rgb(91, 255, 220);
const CHANNEL_B: Color = Color::rgb(255, 196, 68);
const TRIGGER: Color = Color::rgb(166, 124, 255);
const MAX_WAVE_SAMPLES: usize = 480;
const UART_MESSAGE: &[u8] = b"HELLO WORLD";
const UART_BITS_PER_SYMBOL: i32 = 10;
const UART_SYMBOL_MS: u32 = 48;
const UART_RISING_OFFSET: i32 = 9;
const UART_FALLING_OFFSET: i32 = 0;
const PERSISTENCE_FRAMES: u8 = 7;
const PERSISTENCE_STEP_MS: u32 = 13;
const CONTROL_TOP: i32 = 382;
const CONTROL_HEIGHT: i32 = 58;
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

fn uart_bit(index: i32) -> bool {
    let bit_count = UART_MESSAGE.len() as i32 * UART_BITS_PER_SYMBOL;
    let wrapped = index.rem_euclid(bit_count);
    let byte = UART_MESSAGE[(wrapped / UART_BITS_PER_SYMBOL) as usize];
    match wrapped % UART_BITS_PER_SYMBOL {
        0 => false,
        1..=8 => byte & (1 << ((wrapped % UART_BITS_PER_SYMBOL) - 1)) != 0,
        _ => true,
    }
}

fn uart_level(index: i32) -> Fixed {
    if uart_bit(index) {
        Fixed::ONE
    } else {
        -Fixed::ONE
    }
}

fn bandwidth_limited_step(distance: Fixed) -> Fixed {
    let transition_half = Fixed::from_ratio(1, 4);
    let mut response = if distance <= -transition_half {
        Fixed::ZERO
    } else if distance >= transition_half {
        Fixed::ONE
    } else {
        let t = (distance + transition_half) / (transition_half * Fixed::from_int(2));
        let t2 = t * t;
        let t3 = t2 * t;
        t3 * (t * (t * Fixed::from_int(6) - Fixed::from_int(15)) + Fixed::from_int(10))
    };

    if distance > transition_half && distance < Fixed::ONE {
        let ring_distance = distance - transition_half;
        let ring_span = Fixed::ONE - transition_half;
        let envelope = Fixed::ONE - ring_distance / ring_span;
        response += Fixed::sin_deg(ring_distance / ring_span * Fixed::from_int(900))
            * envelope
            * envelope
            * Fixed::from_ratio(9, 100);
    } else if distance < -transition_half && distance > -Fixed::from_ratio(3, 4) {
        let ring_distance = -distance - transition_half;
        let ring_span = Fixed::from_ratio(1, 2);
        let envelope = Fixed::ONE - ring_distance / ring_span;
        response -= Fixed::sin_deg(ring_distance / ring_span * Fixed::from_int(540))
            * envelope
            * envelope
            * Fixed::from_ratio(3, 100);
    }
    response
}

const fn housing_texture_options() -> MirxTextureOptions {
    MirxTextureOptions::new()
        .with_limits(mirx::reader::PayloadLimits::EMBEDDED.with_max_decoded_bytes(800 * 480 * 2))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TriggerEdge {
    #[default]
    Rising,
    Falling,
}

impl TriggerEdge {
    const fn toggled(self) -> Self {
        match self {
            Self::Rising => Self::Falling,
            Self::Falling => Self::Rising,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScopeState {
    running: bool,
    channel_b: bool,
    time_scale: u8,
    gain_a: u8,
    trigger_level: Fixed,
    trigger_edge: TriggerEdge,
    trace_clock_ms: u32,
    revision: u32,
}

impl Default for ScopeState {
    fn default() -> Self {
        Self {
            running: true,
            channel_b: true,
            time_scale: 2,
            gain_a: 2,
            trigger_level: Fixed::from_ratio(3, 10),
            trigger_edge: TriggerEdge::Rising,
            trace_clock_ms: 0,
            revision: 0,
        }
    }
}

impl ScopeState {
    fn base_angle(self, channel: u8, unit_x: Fixed) -> Fixed {
        let cycles = Fixed::from_int(i32::from(self.time_scale + 4));
        if channel == 0 {
            let gain = Fixed::from_ratio(i32::from(self.gain_a + 1), 3);
            let level = (self.trigger_level / gain)
                .clamp(Fixed::from_ratio(-9, 10), Fixed::from_ratio(9, 10));
            let cosine = (Fixed::ONE - level * level).sqrt();
            let signed_cosine = match self.trigger_edge {
                TriggerEdge::Rising => cosine,
                TriggerEdge::Falling => -cosine,
            };
            let trigger_angle =
                Fixed::atan2(level, signed_cosine) * Fixed::from_int(180) / Fixed::PI;
            (unit_x - Fixed::from_ratio(1, 2)) * Fixed::from_int(360) * cycles + trigger_angle
        } else {
            unit_x * Fixed::from_int(360) * cycles + Fixed::from_int(74)
        }
    }

    fn sample_at_angle(self, channel: u8, angle: Fixed) -> Fixed {
        let sample = match channel {
            1 => {
                if Fixed::sin_deg(angle) >= Fixed::ZERO {
                    Fixed::from_ratio(4, 5)
                } else {
                    Fixed::from_ratio(-4, 5)
                }
            }
            _ => {
                let reference = self.base_angle(0, Fixed::from_ratio(1, 2));
                let second = Fixed::sin_deg(angle * Fixed::from_int(2) + Fixed::from_int(31))
                    - Fixed::sin_deg(reference * Fixed::from_int(2) + Fixed::from_int(31));
                let third = Fixed::sin_deg(angle * Fixed::from_int(3) - Fixed::from_int(19))
                    - Fixed::sin_deg(reference * Fixed::from_int(3) - Fixed::from_int(19));
                let interference =
                    Fixed::sin_deg(angle * Fixed::from_ratio(7, 5) + Fixed::from_int(13))
                        - Fixed::sin_deg(reference * Fixed::from_ratio(7, 5) + Fixed::from_int(13));
                Fixed::sin_deg(angle)
                    + second * Fixed::from_ratio(11, 100)
                    + third * Fixed::from_ratio(6, 100)
                    + interference * Fixed::from_ratio(7, 100)
            }
        };
        let gain = if channel == 0 { self.gain_a } else { 1 };
        sample * Fixed::from_ratio(i32::from(gain + 1), 3)
    }

    fn trace_time(self, history: u8) -> Fixed {
        const PERIOD_MS: u32 = 3_600;
        let history_ms = u32::from(history) * PERSISTENCE_STEP_MS;
        let now = self.trace_clock_ms % PERIOD_MS;
        Fixed::from_int(((now + PERIOD_MS - history_ms % PERIOD_MS) % PERIOD_MS) as i32)
    }

    fn trace_sample(self, channel: u8, unit_x: Fixed, history: u8) -> Fixed {
        let time = self.trace_time(history);
        let spatial = unit_x * Fixed::from_int(if channel == 0 { 1_440 } else { 2_160 });
        let jitter_angle = Fixed::sin_deg(time * Fixed::from_int(7) + spatial)
            * if channel == 0 {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::from_int(5)
            };
        let angle = self.base_angle(channel, unit_x) + jitter_angle;
        let mut sample = self.sample_at_angle(channel, angle);
        if channel == 0 {
            let center = Fixed::from_ratio(1, 2);
            let shape_phase = time * Fixed::from_int(2);
            let local_modulation = Fixed::sin_deg(spatial + shape_phase)
                - Fixed::sin_deg(center * Fixed::from_int(1_440) + shape_phase);
            let wander_phase = time / Fixed::from_int(4);
            let baseline_wander = Fixed::sin_deg(unit_x * Fixed::from_int(270) + wander_phase)
                - Fixed::sin_deg(center * Fixed::from_int(270) + wander_phase);
            sample += sample * local_modulation * Fixed::from_ratio(5, 100)
                + baseline_wander * Fixed::from_ratio(3, 100);
            sample += Fixed::sin_deg(angle * Fixed::from_int(11) + time * Fixed::from_int(13))
                * Fixed::from_ratio(2, 100);
        }
        sample
    }

    fn sweep_jitter(self, channel: u8, history: u8) -> Fixed {
        let time = self.trace_time(history);
        Fixed::sin_deg(time * Fixed::from_int(5) + Fixed::from_int(i32::from(channel) * 97))
            * Fixed::from_ratio(3, 4)
    }

    fn uart_symbol_index(self, history: u8) -> usize {
        let history_ms = u32::from(history) * PERSISTENCE_STEP_MS;
        let acquisition_ms = self.trace_clock_ms.saturating_sub(history_ms);
        (acquisition_ms / UART_SYMBOL_MS) as usize % UART_MESSAGE.len()
    }

    fn uart_anchor(self, history: u8) -> i32 {
        let symbol = self.uart_symbol_index(history) as i32;
        symbol * UART_BITS_PER_SYMBOL
            + match self.trigger_edge {
                TriggerEdge::Rising => UART_RISING_OFFSET,
                TriggerEdge::Falling => UART_FALLING_OFFSET,
            }
    }

    fn uart_label(self) -> &'static str {
        match self.uart_symbol_index(0) {
            0 => "UART · H · 01001000",
            1 => "UART · E · 01000101",
            2 | 3 | 9 => "UART · L · 01001100",
            4 | 7 => "UART · O · 01001111",
            5 => "UART · SPACE · 00100000",
            6 => "UART · W · 01010111",
            8 => "UART · R · 01010010",
            _ => "UART · D · 01000100",
        }
    }

    fn uart_wave_sample(self, relative_bits: Fixed, history: u8) -> Fixed {
        let time = self.trace_time(history);
        let local_jitter =
            (Fixed::sin_deg(time * Fixed::from_int(9) + relative_bits * Fixed::from_int(83))
                - Fixed::sin_deg(time * Fixed::from_int(9)))
                * Fixed::from_ratio(3, 50);
        let position = relative_bits + local_jitter;
        let anchor = self.uart_anchor(history);
        let first_bit = position.floor().to_int() - 3;
        let mut signal = uart_level(anchor + first_bit);
        for boundary in first_bit + 1..=first_bit + 7 {
            let before = uart_level(anchor + boundary - 1);
            let after = uart_level(anchor + boundary);
            if before != after {
                signal +=
                    (after - before) * bandwidth_limited_step(position - Fixed::from_int(boundary));
            }
        }
        let noise = Fixed::sin_deg(position * Fixed::from_int(1_937) + time * Fixed::from_int(29))
            * Fixed::from_ratio(1, 100);
        (signal + noise).clamp(Fixed::from_ratio(-6, 5), Fixed::from_ratio(6, 5))
            * Fixed::from_ratio(3, 5)
    }

    fn tick(&mut self, delta_ms: u16) -> bool {
        if !self.running || delta_ms == 0 {
            return false;
        }
        self.trace_clock_ms = self.trace_clock_ms.wrapping_add(u32::from(delta_ms));
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[derive(Clone)]
struct ScopeModel {
    state: Signal<ScopeState>,
}

impl Default for ScopeModel {
    fn default() -> Self {
        Self {
            state: Signal::new(ScopeState::default()),
        }
    }
}

impl ScopeModel {
    fn snapshot(&self) -> ScopeState {
        self.state.get_untracked()
    }

    fn update(&self, update: impl FnOnce(&mut ScopeState)) {
        let mut state = self.snapshot();
        update(&mut state);
        state.revision = state.revision.wrapping_add(1);
        self.state.set(state);
    }
}

#[derive(Clone, Copy)]
enum ScopeAction {
    ToggleRun,
    ToggleChannelB,
    Stop,
    ToggleEdge,
    TimeScale,
    GainA,
}

impl ScopeAction {
    fn publish(self, model: &ScopeModel) {
        model.update(|state| match self {
            Self::ToggleRun => state.running = !state.running,
            Self::ToggleChannelB => state.channel_b = !state.channel_b,
            Self::Stop => state.running = false,
            Self::ToggleEdge => state.trigger_edge = state.trigger_edge.toggled(),
            Self::TimeScale => state.time_scale = (state.time_scale + 1) % 4,
            Self::GainA => state.gain_a = (state.gain_a + 1) % 4,
        });
    }
}

#[derive(Default)]
struct ScopeCanvas;

struct ScopeWaveScratch {
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

fn scope_view() -> View {
    View::new("ScopeCanvas", 60, scope_render).with_filter::<ScopeCanvas>()
}

fn model(cx: &mut crate::ui::UiScope<'_>) -> ScopeModel {
    cx.world_mut()
        .resource::<ScopeModel>()
        .cloned()
        .expect("Signal Scope model")
}

fn centered() -> ParagraphStyle {
    let mut paragraph = ParagraphStyle::label();
    paragraph.letter_spacing = Fixed::from_ratio(1, 2);
    paragraph
}

#[derive(Clone, Copy)]
enum ScopeReadout {
    Blank,
    Acquire,
    ChannelA,
    ChannelB,
    Time,
    Trigger,
}

impl ScopeReadout {
    fn text(self, state: ScopeState) -> &'static str {
        match self {
            Self::Blank => "",
            Self::Acquire => {
                if state.running {
                    "RUN"
                } else {
                    "HOLD"
                }
            }
            Self::ChannelA => "500 mV",
            Self::ChannelB => {
                if state.channel_b {
                    "500 mV"
                } else {
                    "OFF"
                }
            }
            Self::Time => match state.time_scale {
                0 => "20 ms/div",
                1 => "10 ms/div",
                2 => "5 ms/div",
                _ => "2 ms/div",
            },
            Self::Trigger => {
                if state.trigger_edge == TriggerEdge::Rising {
                    "RISING"
                } else {
                    "FALLING"
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ButtonControlSpec {
    left_px: i32,
    width_px: i32,
    readout: ScopeReadout,
    accent: Color,
}

#[derive(Clone, Copy)]
struct ChannelControlSpec {
    left_px: i32,
    width_px: i32,
    label_width_px: i32,
    value_width_px: i32,
    label: &'static str,
    readout: ScopeReadout,
    accent: Color,
}

#[derive(Clone, Copy)]
struct StepControlSpec {
    left_px: i32,
    width_px: i32,
    value_width_px: i32,
    text_height: Dimension,
    readout: ScopeReadout,
}

const fn x_dimension(px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, 800))
}

const fn y_dimension(px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, 480))
}

const fn child_percent(px: i32, parent_px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, parent_px))
}

#[compose]
fn compose_button_control(
    spec: ButtonControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(48)).to_int().clamp(9, 16) as u16
                },
                text_color: spec.accent,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
fn compose_channel_control(
    spec: ChannelControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                spec.label,
                position: Position::Absolute,
                left: 0, top: 0,
                width: child_percent(spec.label_width_px, spec.width_px), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(54)).to_int().clamp(9, 14) as u16
                },
                text_color: spec.accent,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: child_percent(spec.label_width_px, spec.width_px), top: 0,
                width: child_percent(spec.value_width_px, spec.width_px), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(50)).to_int().clamp(9, 15) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
fn compose_step_control(
    spec: StepControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: child_percent(spec.value_width_px, spec.width_px), height: spec.text_height,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(50)).to_int().clamp(9, 15) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
pub fn build_widgets() {
    let model = model(cx);
    let canvas_revision = model.state.clone();
    let controls_state = model.state.clone();
    let uart_state = controls_state.clone();
    let run_action = model.clone();
    let wave_action = model.clone();
    let gain_a_action = model.clone();
    let channel_b_action = model.clone();
    let time_action = model.clone();
    let trigger_action = model;
    let toggle_run = ScopeAction::ToggleRun;
    let stop_scope = ScopeAction::Stop;
    let gain_a = ScopeAction::GainA;
    let toggle_channel_b = ScopeAction::ToggleChannelB;
    let time_scale = ScopeAction::TimeScale;
    let toggle_edge = ScopeAction::ToggleEdge;
    let acquire_control = ButtonControlSpec {
        left_px: 20,
        width_px: 78,
        readout: ScopeReadout::Acquire,
        accent: CHANNEL_A,
    };
    let channel_a_control = ChannelControlSpec {
        left_px: 193,
        width_px: 167,
        label_width_px: 56,
        value_width_px: 74,
        label: "CH A",
        readout: ScopeReadout::ChannelA,
        accent: CHANNEL_A,
    };
    let channel_b_control = ChannelControlSpec {
        left_px: 370,
        width_px: 162,
        label_width_px: 55,
        value_width_px: 72,
        label: "CH B",
        readout: ScopeReadout::ChannelB,
        accent: CHANNEL_B,
    };
    let time_readout = ScopeReadout::Time;
    let trigger_readout = ScopeReadout::Trigger;
    let full_control_height = Dimension::percent(100);
    let trigger_text_height = Dimension::percent(56);
    let time_control = StepControlSpec {
        left_px: 552,
        width_px: 118,
        value_width_px: 82,
        text_height: full_control_height,
        readout: time_readout,
    };
    let trigger_control = StepControlSpec {
        left_px: 680,
        width_px: 106,
        value_width_px: 71,
        text_height: trigger_text_height,
        readout: trigger_readout,
    };
    ui! {
        View (
            id: "scope_shell",
            grow: 1.0,
            bg_color: BG,
            clip_children: true
        ) {
            Image (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                src: "signal_scope_housing"
            ) [IgnoreHitTest]
            ScopeCanvas (
                id: "scope_canvas",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                min_height: 150,
                render_key: ${ u64::from(canvas_revision.get().revision) }
            ) [IgnoreHitTest]
            Text (
                "SIGNAL SCOPE",
                position: Position::Absolute,
                left: Dimension::percent(4), top: Dimension::percent(1),
                width: Dimension::percent(20), height: Dimension::percent(6),
                font: FontToken::Heading,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(44)).to_int().clamp(11, 19) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH A",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(27),
                width: Dimension::percent(6), height: Dimension::percent(6),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(48)).to_int().clamp(10, 17) as u16 },
                text_color: CHANNEL_A,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH B",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(55),
                width: Dimension::percent(6), height: Dimension::percent(6),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(48)).to_int().clamp(10, 17) as u16 },
                text_color: CHANNEL_B,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                text: ${ uart_state.get().uart_label() },
                position: Position::Absolute,
                left: Dimension::percent(71), top: Dimension::percent(68),
                width: Dimension::percent(24), height: Dimension::percent(4),
                font: FontToken::Mono,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(80)).to_int().clamp(7, 11) as u16
                },
                text_color: CHANNEL_B,
                paragraph: centered()
            ) [IgnoreHitTest]
            compose_button_control (acquire_control, controls_state.clone(), toggle_run, run_action)
            compose_button_control (ButtonControlSpec { left_px: 108, width_px: 62, readout: ScopeReadout::Blank, accent: ICE }, controls_state.clone(), stop_scope, wave_action)
            compose_channel_control (channel_a_control, controls_state.clone(), gain_a, gain_a_action)
            compose_channel_control (channel_b_control, controls_state.clone(), toggle_channel_b, channel_b_action)
            compose_step_control (time_control, controls_state.clone(), time_scale, time_action)
            compose_step_control (trigger_control, controls_state, toggle_edge, trigger_action)
        }
    };
}

#[mirui_macros::system(order = ANIMATION)]
fn scope_animation_system(world: &mut World) {
    let delta = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    let Some(model) = world.resource::<ScopeModel>().cloned() else {
        return;
    };
    let mut state = model.snapshot();
    if state.tick(delta) {
        model.state.set(state);
    }
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    register_fonts(&mut app.world);
    app.world.insert_resource(ScopeModel::default());
    app.world.insert_resource(ScopeWaveScratch::default());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(
            crate::app::plugins::ImageResourcesPlugin::empty().with_mirx_bytes_options(
                "signal_scope_housing",
                HOUSING_ART,
                housing_texture_options(),
            ),
        )
        .with_widget(scope_view())
        .add_system(scope_animation_system::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::{TexBuf, Texture};
    use crate::ui::ComputedRect;

    fn stable_sample(state: ScopeState, channel: u8, unit_x: Fixed) -> Fixed {
        state.sample_at_angle(channel, state.base_angle(channel, unit_x))
    }

    fn assert_layout(width: u16, height: u16) {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        super::super::assert_text_layouts_fit(&app.world);
        let canvas = app.world.find_by_id("scope_canvas").unwrap();
        let rect = app.world.get::<ComputedRect>(canvas).unwrap().0;
        assert!(rect.w > Fixed::from_int(100), "{width}x{height}: {rect:?}");
        assert!(rect.h >= Fixed::from_int(150), "{width}x{height}: {rect:?}");
    }

    #[test]
    fn held_scope_keeps_trace_stable() {
        let mut state = ScopeState::default();
        state.running = false;
        let trace_clock_ms = state.trace_clock_ms;
        assert!(!state.tick(16));
        assert_eq!(state.trace_clock_ms, trace_clock_ms);
    }

    #[test]
    fn stop_and_trigger_actions_are_stable() {
        let mut state = ScopeState::default();
        state.trace_clock_ms = 80;
        let model = ScopeModel {
            state: Signal::new(state),
        };
        ScopeAction::Stop.publish(&model);
        assert_eq!(model.snapshot().trace_clock_ms, 80);
        assert_eq!(TriggerEdge::Rising.toggled().toggled(), TriggerEdge::Rising);
    }

    #[test]
    fn acquisition_clock_does_not_move_the_triggered_phase() {
        let mut state = ScopeState::default();
        let positions = [
            Fixed::from_ratio(1, 4),
            Fixed::from_ratio(1, 2),
            Fixed::from_ratio(3, 4),
        ];
        let before = positions.map(|position| stable_sample(state, 0, position));
        assert!(state.tick(250));
        let after = positions.map(|position| stable_sample(state, 0, position));
        assert_eq!(before, after);
    }

    #[test]
    fn phosphor_traces_only_deviate_slightly_from_triggered_signal() {
        let mut state = ScopeState::default();
        state.trace_clock_ms = 1_337;
        for position in [
            Fixed::from_ratio(1, 5),
            Fixed::from_ratio(1, 2),
            Fixed::from_ratio(4, 5),
        ] {
            let stable = stable_sample(state, 0, position);
            for history in 0..=3 {
                let trace = state.trace_sample(0, position, history);
                assert!((trace - stable).abs() <= Fixed::from_ratio(1, 4));
            }
        }
    }

    #[test]
    fn uart_trigger_anchors_match_requested_edges() {
        for symbol in 0..UART_MESSAGE.len() as i32 {
            let frame = symbol * UART_BITS_PER_SYMBOL;
            let rising = frame + UART_RISING_OFFSET;
            let falling = frame + UART_FALLING_OFFSET;
            assert!(!uart_bit(rising - 1));
            assert!(uart_bit(rising));
            assert!(uart_bit(falling - 1));
            assert!(!uart_bit(falling));
        }

        let mut state = ScopeState::default();
        assert_eq!(state.uart_anchor(0), UART_RISING_OFFSET);
        assert!(
            state.uart_wave_sample(Fixed::from_ratio(-1, 100), 0)
                < state.uart_wave_sample(Fixed::from_ratio(1, 100), 0)
        );
        state.trigger_edge = TriggerEdge::Falling;
        assert_eq!(state.uart_anchor(0), UART_FALLING_OFFSET);
        assert!(
            state.uart_wave_sample(Fixed::from_ratio(-1, 100), 0)
                > state.uart_wave_sample(Fixed::from_ratio(1, 100), 0)
        );
    }

    #[test]
    fn uart_edges_have_finite_slew_and_ringing() {
        let before = bandwidth_limited_step(Fixed::from_ratio(-1, 10));
        let center = bandwidth_limited_step(Fixed::ZERO);
        let after = bandwidth_limited_step(Fixed::from_ratio(1, 10));
        assert!(before > Fixed::ZERO && before < center);
        assert_eq!(center, Fixed::from_ratio(1, 2));
        assert!(after > center && after < Fixed::ONE);
        assert!(bandwidth_limited_step(Fixed::from_ratio(35, 100)) > Fixed::ONE);
        assert!(bandwidth_limited_step(Fixed::from_ratio(-35, 100)) < Fixed::ZERO);
    }

    #[test]
    fn uart_acquisition_advances_one_character_at_a_time() {
        let mut state = ScopeState::default();
        assert_eq!(state.uart_symbol_index(0), 0);
        assert_eq!(state.uart_label(), "UART · H · 01001000");

        assert!(state.tick(UART_SYMBOL_MS as u16));
        assert_eq!(state.uart_symbol_index(0), 1);
        assert_eq!(state.uart_label(), "UART · E · 01000101");
        assert_eq!(
            state.uart_anchor(0),
            UART_BITS_PER_SYMBOL + UART_RISING_OFFSET
        );
    }

    #[test]
    fn trigger_edge_controls_center_crossing_direction() {
        let mut state = ScopeState::default();
        let center = Fixed::from_ratio(1, 2);
        let before = center - Fixed::from_ratio(1, 100);
        let after = center + Fixed::from_ratio(1, 100);

        let rising_before = stable_sample(state, 0, before);
        let rising_center = stable_sample(state, 0, center);
        let rising_after = stable_sample(state, 0, after);
        assert!(rising_before < rising_center && rising_center < rising_after);
        assert!((rising_center - state.trigger_level).abs() <= Fixed::from_ratio(1, 50));

        state.trigger_edge = TriggerEdge::Falling;
        let falling_before = stable_sample(state, 0, before);
        let falling_center = stable_sample(state, 0, center);
        let falling_after = stable_sample(state, 0, after);
        assert!(falling_before > falling_center && falling_center > falling_after);
        assert!((falling_center - state.trigger_level).abs() <= Fixed::from_ratio(1, 50));
    }

    #[test]
    fn scope_actions_update_acquisition_controls() {
        let model = ScopeModel::default();
        ScopeAction::ToggleRun.publish(&model);
        ScopeAction::Stop.publish(&model);
        ScopeAction::TimeScale.publish(&model);
        ScopeAction::ToggleChannelB.publish(&model);

        let state = model.snapshot();
        assert!(!state.running);
        assert_eq!(state.trace_clock_ms, 0);
        assert_eq!(state.time_scale, 3);
        assert!(!state.channel_b);
    }

    #[test]
    fn scope_controls_reflow_without_text_overflow() {
        for (width, height) in [(800, 480), (480, 320), (320, 320)] {
            assert_layout(width, height);
        }
    }

    #[test]
    fn housing_art_uses_lossless_lz4_storage() {
        let meta = Texture::probe_mirx(HOUSING_ART).unwrap();
        assert_eq!((meta.width, meta.height), (800, 480));

        let reader = mirx::Reader::open(HOUSING_ART).unwrap();
        let image = reader.primary().unwrap().unwrap().image().unwrap().unwrap();
        let encoded = image.encoded().unwrap();
        assert_eq!(
            encoded.codings().get(0).unwrap().id(),
            mirx::coding::CodingId::LZ4
        );
        assert!(HOUSING_ART.len() < 200 * 1024);

        let texture = Texture::from_mirx_with(HOUSING_ART, housing_texture_options()).unwrap();
        assert!(matches!(&texture.buf, TexBuf::Aligned(_)));
    }
}
