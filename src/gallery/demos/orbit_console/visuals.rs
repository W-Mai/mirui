use alloc::borrow::Cow;
use alloc::vec;

use super::state::ConsoleState;
use super::style::{BG, BLUE, BORDER, DATA_AMBER, MINT, SURFACE, TEXT, TEXT_MUTED, VIOLET};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::render::scene::{GradientStop, GradientUnits, Paint, RadialGradient, SpreadMode};
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::ViewCtx;

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

pub(super) static UNIT_CIRCLE: Path = path!(
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

#[crate::component]
pub(super) struct ConsoleBackdrop {
    mint: Paint,
    violet: Paint,
}

impl ConsoleBackdrop {
    pub(super) fn new() -> Self {
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

#[crate::component(bind(model))]
pub(super) struct OrbitInstrument {
    pub(super) model: ConsoleState,
    core_paints: [Paint; 3],
}

impl OrbitInstrument {
    pub(super) fn new(model: <ConsoleState as crate::core::model::BindType>::Shared) -> Self {
        Self {
            model,
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
        state: &ConsoleState,
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

#[crate::component(bind(model))]
pub(super) struct SignalMeter {
    pub(super) model: ConsoleState,
}

impl SignalMeter {
    fn render(
        &self,
        painter: &mut DemoPainter<'_>,
        rect: &Rect,
        state: &ConsoleState,
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

#[crate::component(bind(model))]
pub(super) struct ActivityPlot {
    pub(super) model: ConsoleState,
}

impl ActivityPlot {
    fn render(
        &self,
        painter: &mut DemoPainter<'_>,
        rect: &Rect,
        state: &ConsoleState,
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

#[crate::view(
    component = ConsoleBackdrop,
    name = "ConsoleBackdrop",
    priority = 10
)]
pub(super) fn backdrop_render(
    renderer: &mut dyn Renderer,
    component: &ConsoleBackdrop,
    rect: &Rect,
    ctx: &mut ViewCtx,
    theme: &Theme,
) {
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    component.render(&mut painter, rect, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
}

#[crate::view(
    component = OrbitInstrument,
    read(model),
    watch(
        model.mode(),
        model.intensity(),
        model.focused_node(),
        model.phase()
    ),
    name = "OrbitInstrument",
    priority = 60
)]
pub(super) fn orbit_render(
    renderer: &mut dyn Renderer,
    component: &OrbitInstrument,
    model: &ConsoleState,
    rect: &Rect,
    ctx: &mut ViewCtx,
    theme: &Theme,
) {
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    component.render(&mut painter, rect, model, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
}

#[crate::view(
    component = SignalMeter,
    read(model),
    watch(model.mode(), model.intensity()),
    name = "SignalMeter",
    priority = 60
)]
pub(super) fn signal_render(
    renderer: &mut dyn Renderer,
    component: &SignalMeter,
    model: &ConsoleState,
    rect: &Rect,
    ctx: &mut ViewCtx,
    theme: &Theme,
) {
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    component.render(&mut painter, rect, model, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
}

#[crate::view(
    component = ActivityPlot,
    read(model),
    watch(model.mode(), model.phase()),
    name = "ActivityPlot",
    priority = 60
)]
pub(super) fn activity_render(
    renderer: &mut dyn Renderer,
    component: &ActivityPlot,
    model: &ConsoleState,
    rect: &Rect,
    ctx: &mut ViewCtx,
    theme: &Theme,
) {
    let mut painter = DemoPainter::new(renderer, ctx.clip, ctx.transform);
    component.render(&mut painter, rect, model, theme);
    ctx.record(painter.error.map_or(Ok(()), Err));
}
