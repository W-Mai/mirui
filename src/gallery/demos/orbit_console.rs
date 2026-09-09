//! Orbit Console combines mirui's layout, interaction, animation, SDF text,
//! gradients, paths, and fixed-point rendering in one product-style interface.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;
use alloc::vec;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::font::{FontManager, mirx as mirx_font};
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::render::scene::{GradientStop, GradientUnits, Paint, RadialGradient, SpreadMode};
use crate::types::Transform;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Slider, Text};

pub const VIEWPORT: (u16, u16) = (1024, 640);

const FONT_BYTES: &[u8] = include_bytes!("assets/misans_ui.mirx");

const BG: Color = Color::rgb(6, 16, 23);
const SURFACE: Color = Color::rgba(16, 36, 51, 238);
const SURFACE_RAISED: Color = Color::rgba(20, 44, 61, 244);
const BORDER: Color = Color::rgba(89, 132, 156, 72);
const TEXT: Color = Color::rgb(226, 241, 247);
const TEXT_MUTED: Color = Color::rgb(129, 157, 173);
const MINT: Color = Color::rgb(99, 242, 207);
const BLUE: Color = Color::rgb(109, 168, 255);
const VIOLET: Color = Color::rgb(167, 139, 250);
const AMBER: Color = Color::rgb(255, 200, 92);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleMode {
    Orbit,
    Flow,
    Pulse,
}

impl ConsoleMode {
    const fn accent(self) -> Color {
        match self {
            Self::Orbit => MINT,
            Self::Flow => BLUE,
            Self::Pulse => VIOLET,
        }
    }

    const fn stage_background(self) -> Color {
        match self {
            Self::Orbit => SURFACE,
            Self::Flow => Color::rgba(14, 34, 56, 238),
            Self::Pulse => Color::rgba(29, 28, 57, 238),
        }
    }

    const fn panel_background(self) -> Color {
        match self {
            Self::Orbit => SURFACE_RAISED,
            Self::Flow => Color::rgba(18, 42, 65, 244),
            Self::Pulse => Color::rgba(35, 33, 68, 244),
        }
    }

    fn chip_background(self, selected: Self) -> Color {
        if self == selected {
            self.accent()
        } else {
            Color::rgba(12, 29, 42, 220)
        }
    }

    fn chip_foreground(self, selected: Self) -> Color {
        if self == selected { BG } else { TEXT_MUTED }
    }

    fn phase_delta(self, elapsed_ms: u32) -> Fixed {
        let elapsed = elapsed_ms as i32;
        match self {
            Self::Orbit => Fixed::from_ratio(elapsed * 3, 50),
            Self::Flow => Fixed::from_ratio(elapsed * 9, 100),
            Self::Pulse => Fixed::from_ratio(elapsed * 3, 25),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleState {
    pub mode: ConsoleMode,
    pub intensity: u8,
    pub focused_node: u8,
    pub paused: bool,
    phase: Fixed,
    revision: u32,
}

impl ConsoleState {
    pub const fn live() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 68,
            focused_node: 1,
            paused: false,
            phase: Fixed::ZERO,
            revision: 0,
        }
    }

    pub const fn capture() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: 78,
            focused_node: 1,
            paused: false,
            phase: Fixed::from_int(32),
            revision: 0,
        }
    }

    pub const fn phase(&self) -> Fixed {
        self.phase
    }

    pub const fn revision(&self) -> u32 {
        self.revision
    }

    fn select_mode(&mut self, mode: ConsoleMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn cycle_focus(&mut self) {
        self.focused_node = (self.focused_node + 1) % 3;
        self.revision = self.revision.wrapping_add(1);
    }

    fn set_intensity(&mut self, value: Fixed) -> bool {
        let next = value.to_int().clamp(0, 100) as u8;
        if self.intensity == next {
            return false;
        }
        self.intensity = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn toggle_paused(&mut self) {
        self.paused = !self.paused;
        self.revision = self.revision.wrapping_add(1);
    }

    fn advance(&mut self, delta_ms: u16) -> bool {
        if self.paused || delta_ms == 0 {
            return false;
        }
        let elapsed = u32::from(delta_ms.min(100));
        self.phase += self.mode.phase_delta(elapsed);
        while self.phase >= Fixed::from_int(360) {
            self.phase -= Fixed::from_int(360);
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoRunMode {
    Live,
    Capture,
}

#[cfg(test)]
struct ConsoleNodes;

#[cfg(test)]
impl ConsoleNodes {
    const STAGE: &'static str = "orbit_console_stage";
    const SIGNAL: &'static str = "orbit_console_signal";
    const ACTIVITY: &'static str = "orbit_console_activity";
    const MODE_CHIPS: [&'static str; 3] = [
        "orbit_console_mode_orbit",
        "orbit_console_mode_flow",
        "orbit_console_mode_pulse",
    ];
    const SLIDER: &'static str = "orbit_console_intensity";
    const PAUSE: &'static str = "orbit_console_pause";
    const FOCUS_LABEL: &'static str = "orbit_console_focus";
    const INTENSITY_LABEL: &'static str = "orbit_console_intensity_value";
}

#[derive(Clone)]
struct ConsoleModel {
    state: Signal<ConsoleState>,
}

impl ConsoleModel {
    fn new(state: ConsoleState) -> Self {
        Self {
            state: Signal::new(state),
        }
    }

    fn signal(&self) -> Signal<ConsoleState> {
        self.state.clone()
    }

    fn snapshot(&self) -> ConsoleState {
        self.state.get_untracked()
    }

    fn advance(&self, delta_ms: u16) {
        let mut state = self.snapshot();
        if state.advance(delta_ms) {
            self.state.set(state);
        }
    }
}

enum ConsoleAction {
    SelectMode(ConsoleMode),
    CycleFocus,
    SetIntensity(Fixed),
    TogglePaused,
}

impl ConsoleAction {
    fn publish(self, signal: &Signal<ConsoleState>) {
        let mut state = signal.get_untracked();
        let changed = match self {
            Self::SelectMode(mode) => state.select_mode(mode),
            Self::CycleFocus => {
                state.cycle_focus();
                true
            }
            Self::SetIntensity(value) => state.set_intensity(value),
            Self::TogglePaused => {
                state.toggle_paused();
                true
            }
        };
        if changed {
            signal.set(state);
        }
    }
}

struct DemoPainter<'a> {
    renderer: &'a mut dyn Renderer,
    clip: &'a Rect,
    transform: Transform,
}

struct ArcStroke {
    center: Point,
    radius: Fixed,
    start: Fixed,
    end: Fixed,
    color: Color,
    width: Fixed,
    opacity: u8,
}

impl<'a> DemoPainter<'a> {
    fn new(renderer: &'a mut dyn Renderer, clip: &'a Rect, transform: Transform) -> Self {
        Self {
            renderer,
            clip,
            transform,
        }
    }

    fn fill(&mut self, area: Rect, color: Color, radius: Fixed, opa: u8) {
        self.renderer.draw(
            &DrawCommand::Fill {
                area,
                transform: self.transform,
                quad: None,
                color,
                radius,
                opa,
            },
            self.clip,
        );
    }

    fn line(&mut self, p1: Point, p2: Point, color: Color, width: Fixed, opa: u8) {
        self.renderer.draw(
            &DrawCommand::Line {
                p1,
                p2,
                transform: self.transform,
                color,
                width,
                opa,
            },
            self.clip,
        );
    }

    fn arc(&mut self, stroke: ArcStroke) {
        self.renderer.draw(
            &DrawCommand::Arc {
                center: stroke.center,
                transform: self.transform,
                radius: stroke.radius,
                start_angle: stroke.start,
                end_angle: stroke.end,
                color: stroke.color,
                width: stroke.width,
                opa: stroke.opacity,
            },
            self.clip,
        );
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint, translate: Point, opa: u8) {
        let local = Transform::translate(translate.x, translate.y);
        self.renderer.draw(
            &DrawCommand::FillPath {
                path,
                transform: self.transform.compose(&local),
                paint,
                opa,
                fill_rule: crate::render::raster::FillRule::EvenOdd,
            },
            self.clip,
        );
    }
}

fn circle_path(radius: i32) -> Path {
    let radius = Fixed::from_int(radius);
    let control = radius * Fixed::from_ratio(141, 256);
    let mut path = Path::new();
    path.move_to(Point::new(radius, Fixed::ZERO))
        .cubic_to(
            Point::new(radius, control),
            Point::new(control, radius),
            Point::new(Fixed::ZERO, radius),
        )
        .cubic_to(
            Point::new(Fixed::ZERO - control, radius),
            Point::new(Fixed::ZERO - radius, control),
            Point::new(Fixed::ZERO - radius, Fixed::ZERO),
        )
        .cubic_to(
            Point::new(Fixed::ZERO - radius, Fixed::ZERO - control),
            Point::new(Fixed::ZERO - control, Fixed::ZERO - radius),
            Point::new(Fixed::ZERO, Fixed::ZERO - radius),
        )
        .cubic_to(
            Point::new(control, Fixed::ZERO - radius),
            Point::new(radius, Fixed::ZERO - control),
            Point::new(radius, Fixed::ZERO),
        )
        .close();
    path
}

fn radial_paint(inner: Color, middle: Color, outer: Color) -> Paint {
    Paint::RadialGradient(RadialGradient {
        center: Point::new(Fixed::from_ratio(9, 20), Fixed::from_ratio(2, 5)).into(),
        radius: Fixed::from_ratio(7, 10).into(),
        focal: Point::new(Fixed::from_ratio(7, 20), Fixed::from_ratio(3, 10)).into(),
        focal_radius: Fixed::ZERO.into(),
        stops: Cow::Owned(vec![
            GradientStop {
                offset: Fixed::ZERO.into(),
                color: inner.into(),
            },
            GradientStop {
                offset: Fixed::from_ratio(11, 20).into(),
                color: middle.into(),
            },
            GradientStop {
                offset: Fixed::ONE.into(),
                color: outer.into(),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

pub struct ConsoleBackdrop {
    glow: Path,
    mint: Paint,
    violet: Paint,
}

impl ConsoleBackdrop {
    pub fn new() -> Self {
        Self {
            glow: circle_path(280),
            mint: radial_paint(
                Color::rgba(31, 209, 180, 120),
                Color::rgba(13, 105, 123, 70),
                Color::rgba(6, 16, 23, 0),
            ),
            violet: radial_paint(
                Color::rgba(123, 92, 220, 105),
                Color::rgba(50, 65, 145, 60),
                Color::rgba(6, 16, 23, 0),
            ),
        }
    }

    fn render(&self, painter: &mut DemoPainter<'_>, rect: &Rect) {
        painter.fill(*rect, BG, Fixed::ZERO, 255);
        painter.fill_path(
            &self.glow,
            &self.mint,
            Point::new(rect.x + Fixed::from_int(160), rect.y + Fixed::from_int(80)),
            160,
        );
        painter.fill_path(
            &self.glow,
            &self.violet,
            Point::new(
                rect.x + rect.w - Fixed::from_int(80),
                rect.y + rect.h - Fixed::from_int(20),
            ),
            145,
        );
    }
}

impl Default for ConsoleBackdrop {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OrbitInstrument {
    core: Path,
    core_paints: [Paint; 3],
}

impl OrbitInstrument {
    pub fn new() -> Self {
        Self {
            core: circle_path(48),
            core_paints: [
                radial_paint(Color::rgb(236, 255, 251), MINT, Color::rgb(8, 62, 69)),
                radial_paint(Color::rgb(241, 248, 255), BLUE, Color::rgb(13, 45, 91)),
                radial_paint(Color::rgb(251, 246, 255), VIOLET, Color::rgb(54, 27, 96)),
            ],
        }
    }

    fn render(&self, painter: &mut DemoPainter<'_>, rect: &Rect, state: ConsoleState) {
        let x0 = rect.x + Fixed::from_int(20);
        let y0 = rect.y + Fixed::from_int(58);
        let x1 = rect.x + rect.w - Fixed::from_int(20);
        let y1 = rect.y + rect.h - Fixed::from_int(82);

        for column in 0..12 {
            let x = x0 + (x1 - x0) * Fixed::from_ratio(column, 11);
            painter.line(
                Point::new(x, y0),
                Point::new(x, y1),
                Color::rgb(44, 80, 98),
                Fixed::from_ratio(1, 2),
                62,
            );
        }
        for row in 0..7 {
            let y = y0 + (y1 - y0) * Fixed::from_ratio(row, 6);
            painter.line(
                Point::new(x0, y),
                Point::new(x1, y),
                Color::rgb(44, 80, 98),
                Fixed::from_ratio(1, 2),
                54,
            );
        }

        let center = Point::new(
            rect.x + rect.w / Fixed::from_int(2),
            rect.y + Fixed::from_int(258),
        );
        let accent = state.mode.accent();
        let radii = [92, 138, 184];
        for (index, radius) in radii.into_iter().enumerate() {
            let offset = Fixed::from_int(index as i32 * 37);
            painter.arc(ArcStroke {
                center,
                radius: Fixed::from_int(radius),
                start: Fixed::from_int(-28) + offset + state.phase() / Fixed::from_int(8),
                end: Fixed::from_int(246) + offset + state.phase() / Fixed::from_int(8),
                color: if index == state.focused_node as usize {
                    accent
                } else {
                    Color::rgb(73, 111, 130)
                },
                width: if index == state.focused_node as usize {
                    Fixed::from_ratio(3, 2)
                } else {
                    Fixed::from_ratio(3, 4)
                },
                opacity: if index == state.focused_node as usize {
                    210
                } else {
                    104
                },
            });
        }

        for tick in 0..32 {
            let angle = Fixed::from_int(tick * 360 / 32);
            let inner = Fixed::from_int(if tick % 4 == 0 { 177 } else { 180 });
            let outer = Fixed::from_int(186);
            let sin = Fixed::sin_deg(angle);
            let cos = Fixed::cos_deg(angle);
            painter.line(
                Point::new(center.x + cos * inner, center.y + sin * inner),
                Point::new(center.x + cos * outer, center.y + sin * outer),
                if tick % 4 == 0 { accent } else { TEXT_MUTED },
                Fixed::from_ratio(3, 4),
                if tick % 4 == 0 { 165 } else { 72 },
            );
        }

        let intensity = Fixed::from_ratio(state.intensity as i32, 100);
        for halo in (0..4).rev() {
            let radius = Fixed::from_int(54 + halo * 14);
            painter.fill(
                Rect::new(center.x - radius, center.y - radius, radius * 2, radius * 2),
                accent,
                radius,
                (24 + (3 - halo) * 10) as u8,
            );
        }
        painter.fill_path(
            &self.core,
            &self.core_paints[state.mode as usize],
            center,
            255,
        );
        painter.arc(ArcStroke {
            center,
            radius: Fixed::from_int(62),
            start: state.phase(),
            end: state.phase() + Fixed::from_int(110) + intensity * Fixed::from_int(60),
            color: Color::rgb(232, 255, 250),
            width: Fixed::from_int(2),
            opacity: 220,
        });

        let node_colors = [MINT, BLUE, VIOLET];
        let node_radii = [92, 138, 184];
        let node_speeds = [
            Fixed::ONE,
            Fixed::from_ratio(-3, 5),
            Fixed::from_ratio(2, 5),
        ];
        let node_offsets = [
            Fixed::from_int(22),
            Fixed::from_int(154),
            Fixed::from_int(272),
        ];
        for index in 0..3 {
            let angle = node_offsets[index] + state.phase() * node_speeds[index];
            let radius = Fixed::from_int(node_radii[index]);
            let node = Point::new(
                center.x + Fixed::cos_deg(angle) * radius,
                center.y + Fixed::sin_deg(angle) * radius,
            );
            painter.line(
                center,
                node,
                node_colors[index],
                Fixed::from_ratio(1, 2),
                76,
            );
            let halo = Fixed::from_int(if index == state.focused_node as usize {
                15
            } else {
                10
            });
            painter.fill(
                Rect::new(node.x - halo, node.y - halo, halo * 2, halo * 2),
                node_colors[index],
                halo,
                38,
            );
            let dot = Fixed::from_int(if index == state.focused_node as usize {
                6
            } else {
                4
            });
            painter.fill(
                Rect::new(node.x - dot, node.y - dot, dot * 2, dot * 2),
                node_colors[index],
                dot,
                255,
            );
            if index == state.focused_node as usize {
                painter.arc(ArcStroke {
                    center: node,
                    radius: Fixed::from_int(12),
                    start: Fixed::from_int(20),
                    end: Fixed::from_int(322),
                    color: Color::rgb(235, 255, 251),
                    width: Fixed::ONE,
                    opacity: 220,
                });
            }
        }
    }
}

impl Default for OrbitInstrument {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct SignalMeter;

impl SignalMeter {
    fn render(&self, painter: &mut DemoPainter<'_>, rect: &Rect, state: ConsoleState) {
        let center = Point::new(rect.x + Fixed::from_int(224), rect.y + Fixed::from_int(76));
        let accent = state.mode.accent();
        painter.arc(ArcStroke {
            center,
            radius: Fixed::from_int(42),
            start: Fixed::from_int(145),
            end: Fixed::from_int(395),
            color: Color::rgb(43, 73, 89),
            width: Fixed::from_int(6),
            opacity: 150,
        });
        let sweep = Fixed::from_int(145)
            + Fixed::from_ratio(state.intensity as i32, 100) * Fixed::from_int(250);
        painter.arc(ArcStroke {
            center,
            radius: Fixed::from_int(42),
            start: Fixed::from_int(145),
            end: sweep,
            color: accent,
            width: Fixed::from_int(6),
            opacity: 255,
        });
        painter.fill(
            Rect::new(
                center.x - Fixed::from_int(3),
                center.y - Fixed::from_int(3),
                Fixed::from_int(6),
                Fixed::from_int(6),
            ),
            Color::rgb(236, 255, 251),
            Fixed::from_int(3),
            255,
        );
    }
}

#[derive(Default)]
pub struct ActivityPlot;

impl ActivityPlot {
    fn render(&self, painter: &mut DemoPainter<'_>, rect: &Rect, state: ConsoleState) {
        let left = rect.x + Fixed::from_int(20);
        let baseline = rect.y + Fixed::from_int(112);
        let width = rect.w - Fixed::from_int(40);
        let accent = state.mode.accent();
        let mut previous = Point::new(left, baseline);
        for sample in 0..24 {
            let x = left + width * Fixed::from_ratio(sample, 23);
            let angle = state.phase() * Fixed::from_ratio(3, 2) + Fixed::from_int(sample * 23);
            let primary = Fixed::sin_deg(angle) * Fixed::from_int(24);
            let secondary = Fixed::sin_deg(angle * Fixed::from_ratio(5, 3)) * Fixed::from_int(8);
            let point = Point::new(x, baseline - primary - secondary);
            if sample > 0 {
                painter.line(previous, point, accent, Fixed::from_ratio(3, 2), 225);
            }
            previous = point;
        }

        for bar in 0..14 {
            let angle = state.phase() + Fixed::from_int(bar * 31);
            let height = Fixed::from_int(7) + Fixed::sin_deg(angle).abs() * Fixed::from_int(24);
            let x = left + Fixed::from_int(bar * 17);
            painter.fill(
                Rect::new(
                    x,
                    rect.y + Fixed::from_int(143) - height,
                    Fixed::from_int(8),
                    height,
                ),
                if bar % 7 == 0 {
                    AMBER
                } else if bar % 3 == 0 {
                    VIOLET
                } else {
                    BLUE
                },
                Fixed::from_int(4),
                120,
            );
        }
    }
}

fn backdrop_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(backdrop) = world.get::<ConsoleBackdrop>(entity) else {
        return;
    };
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    backdrop.render(&mut painter, rect);
}

fn orbit_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(instrument), Some(state)) = (
        world.get::<OrbitInstrument>(entity),
        world.resource::<ConsoleModel>().map(ConsoleModel::snapshot),
    ) else {
        return;
    };
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    instrument.render(&mut painter, rect, state);
}

fn signal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(meter), Some(state)) = (
        world.get::<SignalMeter>(entity),
        world.resource::<ConsoleModel>().map(ConsoleModel::snapshot),
    ) else {
        return;
    };
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    meter.render(&mut painter, rect, state);
}

fn activity_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(plot), Some(state)) = (
        world.get::<ActivityPlot>(entity),
        world.resource::<ConsoleModel>().map(ConsoleModel::snapshot),
    ) else {
        return;
    };
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    plot.render(&mut painter, rect, state);
}

pub fn backdrop_view() -> View {
    View::new("ConsoleBackdrop", 10, backdrop_render).with_filter::<ConsoleBackdrop>()
}

pub fn orbit_view() -> View {
    View::new("OrbitInstrument", 60, orbit_render).with_filter::<OrbitInstrument>()
}

pub fn signal_view() -> View {
    View::new("SignalMeter", 60, signal_render).with_filter::<SignalMeter>()
}

pub fn activity_view() -> View {
    View::new("ActivityPlot", 60, activity_render).with_filter::<ActivityPlot>()
}

fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    let base = mirx_font::font_from_mirx(
        "Orbit Console",
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("Orbit Console font must decode");
    let mut body = base.clone();
    body.size = 14;
    let mut heading = base.clone();
    heading.size = 24;
    let mut mono = base;
    mono.size = 11;
    manager.add_static(FontToken::Default.cache_key(), body);
    manager.add_static(FontToken::Heading.cache_key(), heading);
    manager.add_static(FontToken::Mono.cache_key(), mono);
}

#[mirui_macros::system(order = ANIMATION)]
pub fn console_animation_system(world: &mut World) {
    let delta_ms = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    if let Some(model) = world.resource::<ConsoleModel>().cloned() {
        model.advance(delta_ms);
    }
}

#[compose]
pub fn build_widgets() {
    let state_signal = cx
        .world_mut()
        .resource::<ConsoleModel>()
        .map(ConsoleModel::signal)
        .expect("Orbit Console model must be installed before composition");
    let state = state_signal.get_untracked();
    let (stage_visual, stage_action) = (state_signal.clone(), state_signal.clone());
    let signal_visual = state_signal.clone();
    let activity_visual = state_signal.clone();
    let focus_text = state_signal.clone();
    let intensity_text = state_signal.clone();
    let (orbit_bg, orbit_fg, orbit_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let (flow_bg, flow_fg, flow_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let (pulse_bg, pulse_fg, pulse_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let slider_action = state_signal.clone();
    let (pause_text, pause_action) = (state_signal.clone(), state_signal);

    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: VIEWPORT.0 as i32,
            height: VIEWPORT.1 as i32
        ) [
            ConsoleBackdrop::new(),
        ]
    };

    ui! {
        View (
            position: Position::Absolute,
            left: 24,
            top: 18,
            width: 40,
            height: 40,
            bg_color: MINT,
            border_radius: 12
        ) {
            View (
                position: Position::Absolute,
                left: 9,
                top: 9,
                width: 22,
                height: 22,
                border_color: BG,
                border_width: 2,
                border_radius: 11
            )
            View (
                position: Position::Absolute,
                left: 18,
                top: 5,
                width: 4,
                height: 30,
                bg_color: BG,
                border_radius: 2
            )
        }
    };

    ui! {
        Text (
            "mirui : ORBIT CONSOLE",
            position: Position::Absolute,
            left: 78,
            top: 16,
            font: FontToken::Heading,
            text_color: TEXT
        )
    };
    ui! {
        Text (
            "fixed point graphics instrument",
            position: Position::Absolute,
            left: 80,
            top: 45,
            font: FontToken::Mono,
            text_color: TEXT_MUTED
        )
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 868,
            top: 24,
            width: 132,
            height: 28,
            bg_color: Color::rgba(18, 69, 62, 190),
            border_color: Color::rgba(99, 242, 207, 130),
            border_width: 1,
            border_radius: 14,
            text: "  CONNECTED",
            font: FontToken::Mono,
            text_color: MINT
        )
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 982,
            top: 34,
            width: 7,
            height: 7,
            bg_color: MINT,
            border_radius: 4
        )
    };

    ui! {
        View (
            id: "orbit_console_stage",
            position: Position::Absolute,
            left: 24,
            top: 88,
            width: 656,
            height: 528,
            bg_color: ${ stage_visual.get().mode.stage_background() },
            border_color: BORDER,
            border_width: 1,
            border_radius: 20,
            clip_children: true
        ) [
            OrbitInstrument::new(),
        ] on Tap { ConsoleAction::CycleFocus.publish(&stage_action); }
        {
            Text (
                "ORBIT FIELD : 03 NODES",
                position: Position::Absolute,
                left: 22,
                top: 18,
                font: FontToken::Mono,
                text_color: TEXT_MUTED
            )
            View (
                position: Position::Absolute,
                left: 512,
                top: 17,
                width: 120,
                height: 26,
                bg_color: Color::rgba(20, 64, 65, 200),
                border_radius: 13,
                text: "LIVE VECTOR",
                font: FontToken::Mono,
                text_color: MINT
            )
            View (
                position: Position::Absolute,
                left: 18,
                top: 448,
                width: 620,
                height: 62,
                bg_color: Color::rgba(8, 24, 34, 224),
                border_color: Color::rgba(84, 122, 143, 64),
                border_width: 1,
                border_radius: 12
            ) {
                Text (
                    "FRAME",
                    position: Position::Absolute,
                    left: 18,
                    top: 10,
                    font: FontToken::Mono,
                    text_color: TEXT_MUTED
                )
                Text (
                    "16.67 ms",
                    position: Position::Absolute,
                    left: 18,
                    top: 30,
                    text_color: TEXT
                )
                Text (
                    "PRECISION",
                    position: Position::Absolute,
                    left: 214,
                    top: 10,
                    font: FontToken::Mono,
                    text_color: TEXT_MUTED
                )
                Text (
                    "Q24.8 : SUBPIXEL",
                    position: Position::Absolute,
                    left: 214,
                    top: 30,
                    text_color: TEXT
                )
                Text (
                    "TARGET",
                    position: Position::Absolute,
                    left: 456,
                    top: 10,
                    font: FontToken::Mono,
                    text_color: TEXT_MUTED
                )
                Text (
                    "CPU : NO FPU",
                    position: Position::Absolute,
                    left: 456,
                    top: 30,
                    text_color: TEXT
                )
            }
        }
    };

    ui! {
        Text (
            text: ${
                match focus_text.get().focused_node {
                    0 => "NODE 01 : ACTIVE",
                    1 => "NODE 02 : ACTIVE",
                    _ => "NODE 03 : ACTIVE",
                }
            },
            id: "orbit_console_focus",
            position: Position::Absolute,
            left: 48,
            top: 400,
            font: FontToken::Mono,
            text_color: MINT
        )
    };

    ui! {
        View (
            id: "orbit_console_signal",
            position: Position::Absolute,
            left: 704,
            top: 88,
            width: 296,
            height: 144,
            bg_color: ${ signal_visual.get().mode.panel_background() },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            SignalMeter,
        ] {
            Text (
                "SIGNAL",
                position: Position::Absolute,
                left: 20,
                top: 18,
                font: FontToken::Mono,
                text_color: TEXT_MUTED
            )
            Text (
                "98 PCT",
                position: Position::Absolute,
                left: 20,
                top: 44,
                font: FontToken::Heading,
                text_color: TEXT
            )
            View (
                position: Position::Absolute,
                left: 20,
                top: 96,
                width: 86,
                height: 24,
                bg_color: Color::rgba(20, 76, 67, 190),
                border_radius: 12,
                text: "  STABLE",
                font: FontToken::Mono,
                text_color: MINT
            )
        }
    };

    ui! {
        View (
            id: "orbit_console_activity",
            position: Position::Absolute,
            left: 704,
            top: 248,
            width: 296,
            height: 184,
            bg_color: ${ activity_visual.get().mode.panel_background() },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            ActivityPlot,
        ] {
            Text (
                "ACTIVITY : LIVE",
                position: Position::Absolute,
                left: 20,
                top: 16,
                font: FontToken::Mono,
                text_color: TEXT_MUTED
            )
            Text (
                "12.4",
                position: Position::Absolute,
                left: 204,
                top: 16,
                font: FontToken::Mono,
                text_color: BLUE
            )
        }
    };

    ui! {
        View (
            position: Position::Absolute,
            left: 704,
            top: 448,
            width: 296,
            height: 168,
            bg_color: SURFACE_RAISED,
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) {
            Text (
                "CONTROL",
                position: Position::Absolute,
                left: 20,
                top: 16,
                font: FontToken::Mono,
                text_color: TEXT_MUTED
            )
            Text (
                "INTENSITY",
                position: Position::Absolute,
                left: 20,
                top: 91,
                font: FontToken::Mono,
                text_color: TEXT_MUTED
            )
        }
    };

    ui! {
        Text (
            text: ${ format!("{}", intensity_text.get().intensity) },
            id: "orbit_console_intensity_value",
            position: Position::Absolute,
            left: 954,
            top: 539,
            font: FontToken::Mono,
            text_color: MINT
        )
    };

    ui! {
        View (
            id: "orbit_console_mode_orbit",
            position: Position::Absolute,
            left: 720,
            top: 495,
            width: 78,
            height: 22,
            bg_color: ${ ConsoleMode::Orbit.chip_background(orbit_bg.get().mode) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 10,
            text: "   ORBIT",
            font: FontToken::Mono,
            text_color: ${ ConsoleMode::Orbit.chip_foreground(orbit_fg.get().mode) }
        ) on Tap { ConsoleAction::SelectMode(ConsoleMode::Orbit).publish(&orbit_action); }
    };
    ui! {
        View (
            id: "orbit_console_mode_flow",
            position: Position::Absolute,
            left: 810,
            top: 495,
            width: 78,
            height: 22,
            bg_color: ${ ConsoleMode::Flow.chip_background(flow_bg.get().mode) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 10,
            text: "    FLOW",
            font: FontToken::Mono,
            text_color: ${ ConsoleMode::Flow.chip_foreground(flow_fg.get().mode) }
        ) on Tap { ConsoleAction::SelectMode(ConsoleMode::Flow).publish(&flow_action); }
    };
    ui! {
        View (
            id: "orbit_console_mode_pulse",
            position: Position::Absolute,
            left: 900,
            top: 495,
            width: 78,
            height: 22,
            bg_color: ${ ConsoleMode::Pulse.chip_background(pulse_bg.get().mode) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 10,
            text: "   PULSE",
            font: FontToken::Mono,
            text_color: ${ ConsoleMode::Pulse.chip_foreground(pulse_fg.get().mode) }
        ) on Tap { ConsoleAction::SelectMode(ConsoleMode::Pulse).publish(&pulse_action); }
    };

    let slider = ui! {
        Slider (
            id: "orbit_console_intensity",
            position: Position::Absolute,
            left: 724,
            top: 558,
            width: 256,
            height: 20
        ) on ValueChanged {
            let _ = old;
            ConsoleAction::SetIntensity(*new).publish(&slider_action);
        }
    };
    if let Some(control) = cx.world_mut().get_mut::<Slider>(slider) {
        control.min = Fixed::ZERO;
        control.max = Fixed::from_int(100);
        control.value = Fixed::from_int(state.intensity as i32);
        control.track_color = Color::rgb(35, 63, 78).into();
        control.fill_color = MINT.into();
        control.thumb_color = TEXT.into();
    }

    ui! {
        View (
            id: "orbit_console_pause",
            position: Position::Absolute,
            left: 884,
            top: 586,
            width: 96,
            height: 24,
            bg_color: Color::rgba(32, 55, 71, 235),
            border_color: BORDER,
            border_width: 1,
            border_radius: 10,
            text: ${ if pause_text.get().paused { "     RESUME" } else { "      PAUSE" } },
            font: FontToken::Mono,
            text_color: TEXT
        ) on Tap { ConsoleAction::TogglePaused.publish(&pause_action); }
    };
}

#[cfg(feature = "std")]
pub fn setup<B, F>(app: &mut App<B, F>, parent: Entity, run_mode: DemoRunMode)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let initial_state = match run_mode {
        DemoRunMode::Live => ConsoleState::live(),
        DemoRunMode::Capture => ConsoleState::capture(),
    };
    app.world.insert_resource(ConsoleModel::new(initial_state));
    register_fonts(&mut app.world);
    if let Some(style) = app.world.get_mut::<Style>(parent) {
        style.set_bg_color(BG);
    }
    app.with_widget(backdrop_view())
        .with_widget(orbit_view())
        .with_widget(signal_view())
        .with_widget(activity_view());
    if run_mode == DemoRunMode::Live {
        if app.world.resource::<MonoClock>().is_none() {
            app.add_plugin(StdInstantClockPlugin);
        }
        app.add_system(console_animation_system::system());
    }
    app.compose(parent, build_widgets);
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    setup(app, parent, DemoRunMode::Live);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::render::font::default_font_manager;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::dirty::Dirty;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::slider::{SliderEvent, SliderHandler};

    fn fixture(state: ConsoleState) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(default_font_manager());
        world.insert_resource(ConsoleModel::new(state));
        let mut views = ViewRegistry::with_builtins();
        views.insert(backdrop_view());
        views.insert(orbit_view());
        views.insert(signal_view());
        views.insert(activity_view());
        world.insert_resource(views);
        register_fonts(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        (world, parent)
    }

    fn state(world: &World) -> ConsoleState {
        world
            .resource::<ConsoleModel>()
            .expect("console model")
            .snapshot()
    }

    #[test]
    fn capture_state_is_stable_and_explicit() {
        let state = ConsoleState::capture();
        assert_eq!(state.mode, ConsoleMode::Orbit);
        assert_eq!(state.intensity, 78);
        assert_eq!(state.focused_node, 1);
        assert_eq!(state.phase(), Fixed::from_int(32));
        assert!(!state.paused);
    }

    #[test]
    fn animation_uses_elapsed_time_and_honors_pause() {
        let mut state = ConsoleState::live();
        assert!(state.advance(50));
        assert_eq!(state.phase(), Fixed::from_int(3));
        assert!(!state.advance(0));
        let revision = state.revision();
        state.toggle_paused();
        assert!(!state.advance(50));
        assert_eq!(state.phase(), Fixed::from_int(3));
        assert_eq!(state.revision(), revision + 1);
    }

    #[test]
    fn animation_system_consumes_framework_delta_time() {
        let mut world = World::new();
        world.insert_resource(ConsoleModel::new(ConsoleState::live()));
        world.insert_resource(DeltaTimeMs(50));

        console_animation_system(&mut world);

        assert_eq!(state(&world).phase(), Fixed::from_int(3));
    }

    #[test]
    fn build_widgets_creates_product_regions_and_controls() {
        let (world, parent) = fixture(ConsoleState::capture());
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|children| children.0.len() >= 8)
        );
        let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
        let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
        let activity = world
            .find_by_id(ConsoleNodes::ACTIVITY)
            .expect("activity id");
        let slider = world.find_by_id(ConsoleNodes::SLIDER).expect("slider id");
        assert!(world.has::<OrbitInstrument>(stage));
        assert!(world.has::<SignalMeter>(signal));
        assert!(world.has::<ActivityPlot>(activity));
        assert!(world.has::<Slider>(slider));
    }

    #[test]
    fn controls_update_shared_state_and_visual_style() {
        let (mut world, _) = fixture(ConsoleState::capture());
        let pulse = world
            .find_by_id(ConsoleNodes::MODE_CHIPS[2])
            .expect("pulse mode id");
        let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
        world.remove::<Dirty>(stage);
        GestureHandler::trigger(
            &mut world,
            pulse,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: pulse,
            },
        );
        assert_eq!(state(&world).mode, ConsoleMode::Pulse);
        flush_signal_dirty(&mut world);
        assert_eq!(
            world
                .get::<Style>(pulse)
                .and_then(|style| style.bg_color)
                .map(|color| color.resolve(&crate::ui::Theme::dark())),
            Some(VIOLET)
        );
        assert!(world.has::<Dirty>(stage));
    }

    #[test]
    fn stage_tap_cycles_focus_without_allocation_state() {
        let (mut world, _) = fixture(ConsoleState::capture());
        let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
        GestureHandler::trigger(
            &mut world,
            stage,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: stage,
            },
        );
        assert_eq!(state(&world).focused_node, 2);
        flush_signal_dirty(&mut world);
        let focus_label = world
            .find_by_id(ConsoleNodes::FOCUS_LABEL)
            .expect("focus label id");
        assert!(
            world
                .get::<Text>(focus_label)
                .is_some_and(|text| text.resolve(&world).contains("NODE 03"))
        );
    }

    #[test]
    fn intensity_and_pause_handlers_update_visible_state() {
        let (mut world, _) = fixture(ConsoleState::capture());
        let slider = world.find_by_id(ConsoleNodes::SLIDER).expect("slider id");
        let intensity_label = world
            .find_by_id(ConsoleNodes::INTENSITY_LABEL)
            .expect("intensity label id");
        let pause = world.find_by_id(ConsoleNodes::PAUSE).expect("pause id");
        let callback = world
            .get::<SliderHandler>(slider)
            .expect("slider handler")
            .on_event
            .clone_out();
        callback.call(
            &mut world,
            slider,
            &SliderEvent::ValueChanged {
                new: Fixed::from_int(42),
                old: Fixed::from_int(78),
            },
        );
        assert_eq!(state(&world).intensity, 42);
        flush_signal_dirty(&mut world);
        assert!(
            world
                .get::<Text>(intensity_label)
                .is_some_and(|text| text.resolve(&world) == "42")
        );

        GestureHandler::trigger(
            &mut world,
            pause,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: pause,
            },
        );
        assert!(state(&world).paused);
        flush_signal_dirty(&mut world);
        assert!(
            world
                .get::<Text>(pause)
                .is_some_and(|text| text.resolve(&world).contains("RESUME"))
        );
    }
}
