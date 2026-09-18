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
#[cfg(any(feature = "std", test))]
use crate::render::font::{Font, FontManager};
use crate::render::path::Path;
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::render::scene::{GradientStop, GradientUnits, Paint, RadialGradient, SpreadMode};
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Text};

pub const VIEWPORT: (u16, u16) = (1024, 640);

#[cfg(any(feature = "std", test))]
const FONT_BYTES: &[u8] = include_bytes!("assets/misans_ui.mirx");

const BG: ColorToken = ColorToken::Surface;
const SURFACE: ColorToken = ColorToken::SurfaceVariant;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const TEXT_MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const MINT: ColorToken = ColorToken::Primary;
const BLUE: ColorToken = ColorToken::Secondary;
const VIOLET: ColorToken = ColorToken::Tertiary;
const DATA_AMBER: Color = Color::rgb(255, 200, 92);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleMode {
    Orbit,
    Flow,
    Pulse,
}

impl ConsoleMode {
    const fn accent(self) -> ColorToken {
        match self {
            Self::Orbit => MINT,
            Self::Flow => BLUE,
            Self::Pulse => VIOLET,
        }
    }

    fn chip_background(self, selected: Self) -> ColorToken {
        if self == selected {
            self.accent()
        } else {
            SURFACE
        }
    }

    fn chip_foreground(self, selected: Self) -> ColorToken {
        if self == selected {
            match self {
                Self::Orbit => ColorToken::OnPrimary,
                Self::Flow => ColorToken::OnSecondary,
                Self::Pulse => ColorToken::OnTertiary,
            }
        } else {
            TEXT_MUTED
        }
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
    const SHELL: &'static str = "orbit_console_shell";
    const WORKSPACE: &'static str = "orbit_console_workspace";
    const INSPECTOR: &'static str = "orbit_console_inspector";
    const STAGE: &'static str = "orbit_console_stage";
    const SIGNAL: &'static str = "orbit_console_signal";
    const ACTIVITY: &'static str = "orbit_console_activity";
    const CONTROLS: &'static str = "orbit_console_controls";
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
    #[cfg(any(feature = "std", test))]
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
    error: Option<RenderError>,
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
            error: None,
        }
    }

    fn draw(&mut self, command: &DrawCommand<'_>) {
        if self.error.is_none() {
            self.error = self
                .renderer
                .submit(&DrawRequest::new(command, *self.clip))
                .err();
        }
    }

    fn fill(&mut self, area: Rect, color: Color, radius: Fixed, opa: u8) {
        self.draw(&DrawCommand::Fill {
            area,
            transform: self.transform,
            quad: None,
            color,
            radius,
            opa,
        });
    }

    fn line(&mut self, p1: Point, p2: Point, color: Color, width: Fixed, opa: u8) {
        self.draw(&DrawCommand::Line {
            p1,
            p2,
            transform: self.transform,
            color,
            width,
            opa,
        });
    }

    fn arc(&mut self, stroke: ArcStroke) {
        self.draw(&DrawCommand::Arc {
            center: stroke.center,
            transform: self.transform,
            radius: stroke.radius,
            start_angle: stroke.start,
            end_angle: stroke.end,
            color: stroke.color,
            width: stroke.width,
            opa: stroke.opacity,
        });
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint, translate: Point, scale: Fixed, opa: u8) {
        let local =
            Transform::translate(translate.x, translate.y).compose(&Transform::scale(scale, scale));
        self.draw(&DrawCommand::FillPath {
            path,
            transform: self.transform.compose(&local),
            paint,
            opa,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        });
    }
}

static UNIT_CIRCLE: Path = path!(
    "M 1 0 C 1 0.55078125 0.55078125 1 0 1 C -0.55078125 1 -1 0.55078125 -1 0 C -1 -0.55078125 -0.55078125 -1 0 -1 C 0.55078125 -1 1 -0.55078125 1 0 Z"
);

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
    mint: Paint,
    violet: Paint,
}

impl ConsoleBackdrop {
    pub fn new() -> Self {
        Self {
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

    fn render(&self, painter: &mut DemoPainter<'_>, rect: &Rect, theme: &Theme) {
        painter.fill(*rect, theme.resolve(BG), Fixed::ZERO, 255);
        painter.fill_path(
            &UNIT_CIRCLE,
            &self.mint,
            Point::new(rect.x + Fixed::from_int(160), rect.y + Fixed::from_int(80)),
            Fixed::from_int(280),
            160,
        );
        painter.fill_path(
            &UNIT_CIRCLE,
            &self.violet,
            Point::new(
                rect.x + rect.w - Fixed::from_int(80),
                rect.y + rect.h - Fixed::from_int(20),
            ),
            Fixed::from_int(280),
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
    core_paints: [Paint; 3],
}

impl OrbitInstrument {
    pub fn new() -> Self {
        Self {
            core_paints: [
                radial_paint(
                    Color::rgb(236, 255, 251),
                    Color::rgb(99, 242, 207),
                    Color::rgb(8, 62, 69),
                ),
                radial_paint(
                    Color::rgb(241, 248, 255),
                    Color::rgb(109, 168, 255),
                    Color::rgb(13, 45, 91),
                ),
                radial_paint(
                    Color::rgb(251, 246, 255),
                    Color::rgb(167, 139, 250),
                    Color::rgb(54, 27, 96),
                ),
            ],
        }
    }

    fn render(
        &self,
        painter: &mut DemoPainter<'_>,
        rect: &Rect,
        state: ConsoleState,
        theme: &Theme,
    ) {
        let surface = theme.resolve(SURFACE);
        let border = theme.resolve(BORDER);
        let text = theme.resolve(TEXT);
        let text_muted = theme.resolve(TEXT_MUTED);
        let grid = surface.blend_with(border, Fixed::from_ratio(1, 2));
        let margin = (rect.w / Fixed::from_int(24))
            .max(Fixed::from_int(8))
            .min(Fixed::from_int(20));
        let header = (rect.h / Fixed::from_int(6))
            .max(Fixed::from_int(38))
            .min(Fixed::from_int(72));
        let footer = (rect.h / Fixed::from_int(8))
            .max(Fixed::from_int(28))
            .min(Fixed::from_int(56));
        let x0 = rect.x + margin;
        let y0 = rect.y + header;
        let x1 = rect.x + rect.w - margin;
        let y1 = rect.y + rect.h - footer;

        for column in 0..12 {
            let x = x0 + (x1 - x0) * Fixed::from_ratio(column, 11);
            painter.line(
                Point::new(x, y0),
                Point::new(x, y1),
                grid,
                Fixed::from_ratio(1, 2),
                62,
            );
        }
        for row in 0..7 {
            let y = y0 + (y1 - y0) * Fixed::from_ratio(row, 6);
            painter.line(
                Point::new(x0, y),
                Point::new(x1, y),
                grid,
                Fixed::from_ratio(1, 2),
                54,
            );
        }

        let center = Point::new(
            rect.x + rect.w / Fixed::from_int(2),
            y0 + (y1 - y0) / Fixed::from_int(2),
        );
        let accent = theme.resolve(state.mode.accent());
        let outer_radius = ((x1 - x0) / Fixed::from_int(2))
            .min((y1 - y0) / Fixed::from_int(2))
            .max(Fixed::from_int(18))
            - Fixed::from_int(5);
        let radii = [
            outer_radius / Fixed::from_int(2),
            outer_radius * Fixed::from_ratio(3, 4),
            outer_radius,
        ];
        for (index, radius) in radii.into_iter().enumerate() {
            let offset = Fixed::from_int(index as i32 * 37);
            painter.arc(ArcStroke {
                center,
                radius,
                start: Fixed::from_int(-28) + offset + state.phase() / Fixed::from_int(8),
                end: Fixed::from_int(246) + offset + state.phase() / Fixed::from_int(8),
                color: if index == state.focused_node as usize {
                    accent
                } else {
                    border
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
            let inner = outer_radius
                - if tick % 4 == 0 {
                    Fixed::from_int(9)
                } else {
                    Fixed::from_int(6)
                };
            let outer = outer_radius + Fixed::from_int(1);
            let sin = Fixed::sin_deg(angle);
            let cos = Fixed::cos_deg(angle);
            painter.line(
                Point::new(center.x + cos * inner, center.y + sin * inner),
                Point::new(center.x + cos * outer, center.y + sin * outer),
                if tick % 4 == 0 { accent } else { text_muted },
                Fixed::from_ratio(3, 4),
                if tick % 4 == 0 { 165 } else { 72 },
            );
        }

        let intensity = Fixed::from_ratio(state.intensity as i32, 100);
        let core_radius = (outer_radius * Fixed::from_ratio(13, 50))
            .max(Fixed::from_int(12))
            .min(Fixed::from_int(48));
        for halo in (0..4).rev() {
            let radius = core_radius + outer_radius * Fixed::from_ratio(halo + 1, 18);
            painter.fill(
                Rect::new(center.x - radius, center.y - radius, radius * 2, radius * 2),
                accent,
                radius,
                (24 + (3 - halo) * 10) as u8,
            );
        }
        painter.fill_path(
            &UNIT_CIRCLE,
            &self.core_paints[state.mode as usize],
            center,
            core_radius,
            255,
        );
        painter.arc(ArcStroke {
            center,
            radius: core_radius + outer_radius / Fixed::from_int(12),
            start: state.phase(),
            end: state.phase() + Fixed::from_int(110) + intensity * Fixed::from_int(60),
            color: text,
            width: Fixed::from_int(2),
            opacity: 220,
        });

        let node_colors = [
            theme.resolve(MINT),
            theme.resolve(BLUE),
            theme.resolve(VIOLET),
        ];
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
            let radius = radii[index];
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
                    color: text,
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
    fn render(
        &self,
        painter: &mut DemoPainter<'_>,
        rect: &Rect,
        state: ConsoleState,
        theme: &Theme,
    ) {
        let inset = (rect.w / Fixed::from_int(14))
            .max(Fixed::from_int(6))
            .min(Fixed::from_int(18));
        let radius = (rect.w.min(rect.h) / Fixed::from_int(4))
            .max(Fixed::from_int(10))
            .min(Fixed::from_int(42));
        let center = Point::new(
            rect.x + rect.w - radius - inset,
            rect.y + rect.h / Fixed::from_int(2),
        );
        let accent = theme.resolve(state.mode.accent());
        painter.arc(ArcStroke {
            center,
            radius,
            start: Fixed::from_int(145),
            end: Fixed::from_int(395),
            color: theme.resolve(BORDER),
            width: Fixed::from_int(6),
            opacity: 150,
        });
        let sweep = Fixed::from_int(145)
            + Fixed::from_ratio(state.intensity as i32, 100) * Fixed::from_int(250);
        painter.arc(ArcStroke {
            center,
            radius,
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
            theme.resolve(TEXT),
            Fixed::from_int(3),
            255,
        );
    }
}

#[derive(Default)]
pub struct ActivityPlot;

impl ActivityPlot {
    fn render(
        &self,
        painter: &mut DemoPainter<'_>,
        rect: &Rect,
        state: ConsoleState,
        theme: &Theme,
    ) {
        let inset = (rect.w / Fixed::from_int(16))
            .max(Fixed::from_int(8))
            .min(Fixed::from_int(20));
        let left = rect.x + inset;
        let width = rect.w - inset * Fixed::from_int(2);
        let plot_top = rect.y + (rect.h / Fixed::from_int(4)).max(Fixed::from_int(24));
        let plot_height = (rect.y + rect.h - plot_top - inset).max(Fixed::from_int(12));
        let baseline = plot_top + plot_height * Fixed::from_ratio(2, 5);
        let accent = theme.resolve(state.mode.accent());
        let mut previous = Point::new(left, baseline);
        for sample in 0..24 {
            let x = left + width * Fixed::from_ratio(sample, 23);
            let angle = state.phase() * Fixed::from_ratio(3, 2) + Fixed::from_int(sample * 23);
            let primary = Fixed::sin_deg(angle) * plot_height * Fixed::from_ratio(3, 10);
            let secondary =
                Fixed::sin_deg(angle * Fixed::from_ratio(5, 3)) * plot_height / Fixed::from_int(10);
            let point = Point::new(x, baseline - primary - secondary);
            if sample > 0 {
                painter.line(previous, point, accent, Fixed::from_ratio(3, 2), 225);
            }
            previous = point;
        }

        for bar in 0..14 {
            let angle = state.phase() + Fixed::from_int(bar * 31);
            let height = Fixed::from_int(3)
                + Fixed::sin_deg(angle).abs() * plot_height * Fixed::from_ratio(3, 10);
            let x = left + width * Fixed::from_ratio(bar, 14);
            painter.fill(
                Rect::new(
                    x,
                    plot_top + plot_height * Fixed::from_ratio(4, 5) - height,
                    (width / Fixed::from_int(28)).max(Fixed::from_int(3)),
                    height,
                ),
                if bar % 7 == 0 {
                    DATA_AMBER
                } else if bar % 3 == 0 {
                    theme.resolve(VIOLET)
                } else {
                    theme.resolve(BLUE)
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
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    backdrop.render(&mut painter, rect, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
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
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    instrument.render(&mut painter, rect, state, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
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
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    meter.render(&mut painter, rect, state, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
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
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    plot.render(&mut painter, rect, state, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
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

#[cfg(any(feature = "std", test))]
fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    let base = Font::from_mirx(
        "Orbit Console",
        14,
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

fn console_signal(cx: &mut crate::ui::UiScope<'_>) -> Signal<ConsoleState> {
    cx.world_mut()
        .resource::<ConsoleModel>()
        .map(ConsoleModel::signal)
        .expect("Orbit Console model must be installed before composition")
}

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            height: 54,
            align: AlignItems::Center,
            column_gap: 10
        ) {
            View (
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
                    border_color: ColorToken::OnPrimary,
                    border_width: 2,
                    border_radius: 11
                )
                View (
                    position: Position::Absolute,
                    left: 18,
                    top: 5,
                    width: 4,
                    height: 30,
                    bg_color: ColorToken::OnPrimary,
                    border_radius: 2
                )
            }
            Column (grow: 1.0, justify: JustifyContent::Center, row_gap: 2) {
                Text (
                    "mirui : ORBIT CONSOLE",
                    font: FontToken::Heading,
                    font_size: 20,
                    text_color: TEXT
                )
                Text (
                    "fixed point graphics instrument",
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT_MUTED
                )
            }
        }
    }
}

#[compose]
fn compose_orbit_stage() -> Entity {
    let state_signal = console_signal(cx);
    let stage_visual = state_signal.clone();
    let stage_action = state_signal.clone();
    let focus_text = state_signal;

    let stage = ui! {
        Column (
            id: "orbit_console_stage",
            grow: 2.0,
            min_width: 260,
            min_height: Dimension::percent(48),
            padding: Padding::all(12),
            row_gap: 6,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 20,
            clip_children: true,
            render_key: ${ u64::from(stage_visual.get().revision()) }
        ) [
            OrbitInstrument::new(),
        ] on Tap { ConsoleAction::CycleFocus.publish(&stage_action); }
        {
            Row (height: 24, align: AlignItems::Center, column_gap: 8) {
                Text (
                    "ORBIT FIELD : 03 NODES",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT_MUTED
                )
                Text (
                    "LIVE VECTOR",
                    width: 92,
                    height: 22,
                    bg_color: SURFACE,
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 11,
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (grow: 1.0)
            Text (
                text: ${
                    match focus_text.get().focused_node {
                        0 => "NODE 01 : ACTIVE",
                        1 => "NODE 02 : ACTIVE",
                        _ => "NODE 03 : ACTIVE",
                    }
                },
                id: "orbit_console_focus",
                height: 20,
                font: FontToken::Mono,
                font_size: 10,
                text_color: MINT
            )
        }
    };
    stage
}

#[compose]
fn compose_signal_card() -> Entity {
    let signal_visual = console_signal(cx);

    ui! {
        Row (
            id: "orbit_console_signal",
            grow: 4.0,
            min_height: 44,
            padding: Padding::all(10),
            align: AlignItems::Center,
            bg_color: SURFACE,
            render_key: ${ u64::from(signal_visual.get().revision()) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            SignalMeter,
        ] {
            Column (grow: 1.0, row_gap: 2) {
                Text (
                    "SIGNAL",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: TEXT_MUTED
                )
                Text (
                    "98 PCT",
                    font: FontToken::Heading,
                    font_size: 20,
                    text_color: TEXT
                )
            }
            View (width: 48)
        }
    }
}

#[compose]
fn compose_activity_card() -> Entity {
    let activity_visual = console_signal(cx);

    ui! {
        Column (
            id: "orbit_console_activity",
            grow: 5.0,
            min_height: 48,
            padding: Padding::all(10),
            bg_color: SURFACE,
            render_key: ${ u64::from(activity_visual.get().revision()) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            ActivityPlot,
        ] {
            Row (height: 18, align: AlignItems::Center) {
                Text (
                    "ACTIVITY : LIVE",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: TEXT_MUTED
                )
                Text (
                    "12.4",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: BLUE
                )
            }
        }
    }
}

#[compose]
fn compose_controls() -> Entity {
    let state_signal = cx
        .world_mut()
        .resource::<ConsoleModel>()
        .map(ConsoleModel::signal)
        .expect("Orbit Console model must be installed before composition");
    let state = state_signal.get_untracked();
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

    let controls = ui! {
        Column (
            id: "orbit_console_controls",
            grow: 4.0,
            min_height: 64,
            padding: Padding::all(10),
            row_gap: 4,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) {
            Row (height: 16, align: AlignItems::Center) {
                Text (
                    "CONTROL · INTENSITY",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: TEXT_MUTED
                )
                Text (
                    text: ${ format!("{}", intensity_text.get().intensity) },
                    id: "orbit_console_intensity_value",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: MINT
                )
            }
            Row (height: 20, column_gap: 4) {
                Button (
                    id: "orbit_console_mode_orbit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Orbit.chip_background(orbit_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Orbit.chip_foreground(orbit_fg.get().mode) }
                ) [
                    Text::label("ORBIT"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Orbit).publish(&orbit_action); }
                Button (
                    id: "orbit_console_mode_flow",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Flow.chip_background(flow_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Flow.chip_foreground(flow_fg.get().mode) }
                ) [
                    Text::label("FLOW"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Flow).publish(&flow_action); }
                Button (
                    id: "orbit_console_mode_pulse",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Pulse.chip_background(pulse_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Pulse.chip_foreground(pulse_fg.get().mode) }
                ) [
                    Text::label("PULSE"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Pulse).publish(&pulse_action); }
            }
            Row (grow: 1.0, min_height: 20, align: AlignItems::Center, column_gap: 6) {
                Slider (
                    id: "orbit_console_intensity",
                    grow: 1.0,
                    height: 18,
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    value: Fixed::from_int(state.intensity as i32),
                    track_color: BORDER,
                    fill_color: MINT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = old;
                    ConsoleAction::SetIntensity(*new).publish(&slider_action);
                }
                Button (
                    id: "orbit_console_pause",
                    size: ButtonSize::Compact,
                    text: ${ if pause_text.get().paused { "RESUME" } else { "PAUSE" } },
                    width: 54,
                    height: 20,
                    normal_color: SURFACE,
                    pressed_color: MINT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: TEXT
                ) on Tap { ConsoleAction::TogglePaused.publish(&pause_action); }
            }
        }
    };
    controls
}

#[compose]
fn compose_inspector() -> Entity {
    ui! {
        Column (
            id: "orbit_console_inspector",
            grow: 1.0,
            min_width: 170,
            min_height: Dimension::percent(48),
            row_gap: 8
        ) {
            compose_signal_card ()
            compose_activity_card ()
            compose_controls ()
        }
    }
}

#[compose]
fn compose_workspace() -> Entity {
    ui! {
        Row (
            id: "orbit_console_workspace",
            grow: 1.0,
            wrap: FlexWrap::Wrap,
            align: AlignItems::Stretch,
            row_gap: 10,
            column_gap: 12
        ) {
            compose_orbit_stage ()
            compose_inspector ()
        }
    }
}

#[compose]
fn compose_status_strip() -> Entity {
    ui! {
        Row (
            height: 28,
            padding: Padding::all(6),
            align: AlignItems::Center,
            column_gap: 8,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) {
            Text ("16.67 ms", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text ("Q24.8", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text ("CPU", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text (
                "CONNECTED",
                width: 72,
                font: FontToken::Mono,
                font_size: 8,
                text_color: MINT,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            id: "orbit_console_shell",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true
        ) [
            ConsoleBackdrop::new(),
        ] {
            compose_header ()
            compose_workspace ()
            compose_status_strip ()
        }
    };
}

#[cfg(feature = "std")]
pub fn setup<B, F>(app: &mut App<B, F>, parent: Entity, run_mode: DemoRunMode)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
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
    use crate::surface::FramebufferAccess;
    use crate::ui::Children;
    use crate::ui::ComputedRect;
    use crate::ui::IdMap;
    use crate::ui::Parent;
    use crate::ui::UiScope;
    use crate::ui::dirty::Dirty;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::Button;
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
    fn shared_circle_geometry_is_static() {
        assert!(UNIT_CIRCLE.is_borrowed());
        assert_eq!(UNIT_CIRCLE.commands().len(), 6);
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
        let shell = world.find_by_id(ConsoleNodes::SHELL).expect("shell id");
        let workspace = world
            .find_by_id(ConsoleNodes::WORKSPACE)
            .expect("workspace id");
        let inspector = world
            .find_by_id(ConsoleNodes::INSPECTOR)
            .expect("inspector id");
        let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
        let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
        let activity = world
            .find_by_id(ConsoleNodes::ACTIVITY)
            .expect("activity id");
        let controls = world
            .find_by_id(ConsoleNodes::CONTROLS)
            .expect("controls id");
        let slider = world.find_by_id(ConsoleNodes::SLIDER).expect("slider id");
        assert_eq!(
            world
                .get::<Children>(parent)
                .map(|children| children.0.len()),
            Some(1)
        );
        assert_eq!(
            world.get::<Parent>(shell).map(|parent| parent.0),
            Some(parent)
        );
        assert_eq!(
            world.get::<Parent>(workspace).map(|parent| parent.0),
            Some(shell)
        );
        assert_eq!(
            world.get::<Parent>(stage).map(|parent| parent.0),
            Some(workspace)
        );
        assert_eq!(
            world.get::<Parent>(inspector).map(|parent| parent.0),
            Some(workspace)
        );
        for card in [signal, activity, controls] {
            assert_eq!(
                world.get::<Parent>(card).map(|parent| parent.0),
                Some(inspector)
            );
        }
        assert!(world.has::<OrbitInstrument>(stage));
        assert!(world.has::<SignalMeter>(signal));
        assert!(world.has::<ActivityPlot>(activity));
        assert!(world.has::<Slider>(slider));
    }

    #[test]
    fn workspace_responds_across_supported_viewports() {
        use crate::types::Viewport;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(320, 568), (480, 320), (768, 480), (1024, 640), (1440, 900)] {
            let (mut world, parent) = fixture(ConsoleState::capture());
            update_layout(
                &mut world,
                parent,
                &Viewport::new(width, height, Fixed::ONE),
            );
            let rect = |world: &World, id| {
                let entity = world.find_by_id(id).expect("responsive region id");
                world.get::<ComputedRect>(entity).expect("computed rect").0
            };
            let workspace = rect(&world, ConsoleNodes::WORKSPACE);
            let stage = rect(&world, ConsoleNodes::STAGE);
            let inspector = rect(&world, ConsoleNodes::INSPECTOR);
            assert!(stage.w > Fixed::ZERO && stage.h > Fixed::ZERO);
            assert!(inspector.w > Fixed::ZERO && inspector.h > Fixed::ZERO);
            assert!(stage.x >= workspace.x && stage.x + stage.w <= workspace.x + workspace.w);
            assert!(
                inspector.x >= workspace.x
                    && inspector.x + inspector.w <= workspace.x + workspace.w
            );
            assert!(stage.y >= workspace.y && stage.y + stage.h <= workspace.y + workspace.h);
            assert!(
                inspector.y >= workspace.y
                    && inspector.y + inspector.h <= workspace.y + workspace.h
            );
            if width == 320 {
                assert!(inspector.y > stage.y);
            } else {
                assert_eq!(inspector.y, stage.y);
            }
        }
    }

    #[test]
    fn controls_update_shared_state_and_visual_style() {
        let (mut world, _) = fixture(ConsoleState::capture());
        let pulse = world
            .find_by_id(ConsoleNodes::MODE_CHIPS[2])
            .expect("pulse mode id");
        let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
        world.remove::<Dirty>(stage);
        world.remove::<crate::ui::dirty::VisualDirty>(stage);
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
                .get::<Button>(pulse)
                .map(|button| button.normal_color.resolve(&Theme::dark())),
            Some(Theme::dark().resolve(VIOLET))
        );
        assert!(!world.has::<Dirty>(stage));
        assert!(world.has::<crate::ui::dirty::VisualDirty>(stage));
    }

    #[test]
    fn shell_and_instruments_resolve_the_active_theme() {
        let render = |theme_id| {
            let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            setup(&mut app, root, DemoRunMode::Capture);
            app.set_root(root);
            app.set_theme(theme_id).unwrap();
            app.render().unwrap();
            <[u8; 3]>::try_from(&app.backend.framebuffer().buf.as_slice()[..3]).unwrap()
        };

        let light = render(crate::gallery::showcase_theme::LIGHT_ID);
        let dark = render(crate::gallery::showcase_theme::DARK_ID);
        assert_ne!(light, dark);
        assert!(
            light.iter().map(|channel| u16::from(*channel)).sum::<u16>()
                > dark.iter().map(|channel| u16::from(*channel)).sum::<u16>()
        );
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
