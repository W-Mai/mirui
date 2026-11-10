use super::state::ConsoleState;
use super::style::{ACCENT, BORDER, SURFACE, TEXT, TEXT_MUTED};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::scene::Paint;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

struct DemoPainter<'paint, 'frame> {
    paint: &'paint mut PaintCtx<'frame>,
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

impl<'paint, 'frame> DemoPainter<'paint, 'frame> {
    fn new(paint: &'paint mut PaintCtx<'frame>, transform: Transform) -> Self {
        Self { paint, transform }
    }

    fn draw(&mut self, command: &DrawCommand<'_>) {
        self.paint.draw(command);
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

    fn disc(&mut self, center: Point, radius: Fixed, color: Color) {
        let local =
            Transform::translate(center.x, center.y).compose(&Transform::scale(radius, radius));
        self.draw(&DrawCommand::FillPath {
            path: &UNIT_CIRCLE,
            transform: self.transform.compose(&local),
            paint: &Paint::Color(color.into()),
            opa: 255,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        });
    }
}

pub(super) static UNIT_CIRCLE: Path = path!(
    "M 1 0 C 1 0.55078125 0.55078125 1 0 1 C -0.55078125 1 -1 0.55078125 -1 0 C -1 -0.55078125 -0.55078125 -1 0 -1 C 0.55078125 -1 1 -0.55078125 1 0 Z"
);

/// Polar plot of three orbiting nodes around a central body.
#[crate::component]
pub(super) struct OrbitInstrument {
    pub(super) model: model!(ConsoleState),
}

impl OrbitInstrument {
    fn render(
        &self,
        painter: &mut DemoPainter<'_, '_>,
        rect: &Rect,
        state: &ConsoleState,
        theme: &Theme,
    ) {
        let surface = theme.resolve(SURFACE);
        let border = theme.resolve(BORDER);
        let text = theme.resolve(TEXT);
        let muted = theme.resolve(TEXT_MUTED);
        let accent = theme.resolve(ACCENT);
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
                90,
            );
        }
        for row in 0..7 {
            let y = y0 + (y1 - y0) * Fixed::from_ratio(row, 6);
            painter.line(
                Point::new(x0, y),
                Point::new(x1, y),
                grid,
                Fixed::from_ratio(1, 2),
                90,
            );
        }

        let center = Point::new(
            rect.x + rect.w / Fixed::from_int(2),
            y0 + (y1 - y0) / Fixed::from_int(2),
        );
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
            let focused = index == state.focused_node as usize;
            let offset = Fixed::from_int(index as i32 * 37);
            painter.arc(ArcStroke {
                center,
                radius,
                start: Fixed::from_int(-28) + offset + state.phase() / Fixed::from_int(8),
                end: Fixed::from_int(246) + offset + state.phase() / Fixed::from_int(8),
                color: if focused { accent } else { muted },
                width: if focused {
                    Fixed::from_ratio(3, 2)
                } else {
                    Fixed::from_ratio(3, 4)
                },
                opacity: if focused { 255 } else { 110 },
            });
        }

        for tick in 0..32 {
            let major = tick % 4 == 0;
            let angle = Fixed::from_int(tick * 360 / 32);
            let inner = outer_radius
                - if major {
                    Fixed::from_int(8)
                } else {
                    Fixed::from_int(4)
                };
            let outer = outer_radius + Fixed::from_int(1);
            let sin = Fixed::sin_deg(angle);
            let cos = Fixed::cos_deg(angle);
            painter.line(
                Point::new(center.x + cos * inner, center.y + sin * inner),
                Point::new(center.x + cos * outer, center.y + sin * outer),
                muted,
                Fixed::from_ratio(3, 4),
                if major { 200 } else { 90 },
            );
        }

        let intensity = Fixed::from_ratio(state.intensity as i32, 100);
        let core_radius = (outer_radius * Fixed::from_ratio(4, 25))
            .max(Fixed::from_int(8))
            .min(Fixed::from_int(30));
        painter.disc(center, core_radius, border);
        painter.disc(center, core_radius / Fixed::from_int(4), text);
        painter.arc(ArcStroke {
            center,
            radius: core_radius + Fixed::from_int(5),
            start: state.phase(),
            end: state.phase() + Fixed::from_int(110) + intensity * Fixed::from_int(60),
            color: accent,
            width: Fixed::from_int(2),
            opacity: 255,
        });

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
            let focused = index == state.focused_node as usize;
            let angle = node_offsets[index] + state.phase() * node_speeds[index];
            let radius = radii[index];
            let node = Point::new(
                center.x + Fixed::cos_deg(angle) * radius,
                center.y + Fixed::sin_deg(angle) * radius,
            );
            if focused {
                painter.line(center, node, accent, Fixed::from_ratio(3, 4), 140);
                painter.disc(node, Fixed::from_int(5), accent);
                painter.arc(ArcStroke {
                    center: node,
                    radius: Fixed::from_int(10),
                    start: Fixed::ZERO,
                    end: Fixed::from_int(360),
                    color: text,
                    width: Fixed::ONE,
                    opacity: 200,
                });
            } else {
                painter.disc(node, Fixed::from_ratio(7, 2), muted);
            }
        }
    }
}

#[crate::component]
pub(super) struct SignalMeter {
    pub(super) model: model!(ConsoleState),
}

impl SignalMeter {
    fn render(
        &self,
        painter: &mut DemoPainter<'_, '_>,
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
        painter.arc(ArcStroke {
            center,
            radius,
            start: Fixed::from_int(145),
            end: Fixed::from_int(395),
            color: theme.resolve(BORDER),
            width: Fixed::from_int(5),
            opacity: 255,
        });
        let sweep = Fixed::from_int(145)
            + Fixed::from_ratio(state.intensity as i32, 100) * Fixed::from_int(250);
        painter.arc(ArcStroke {
            center,
            radius,
            start: Fixed::from_int(145),
            end: sweep,
            color: theme.resolve(ACCENT),
            width: Fixed::from_int(5),
            opacity: 255,
        });
    }
}

/// Packet rate as a line over per-interval bars.
#[crate::component]
pub(super) struct ActivityPlot {
    pub(super) model: model!(ConsoleState),
}

impl ActivityPlot {
    fn render(
        &self,
        painter: &mut DemoPainter<'_, '_>,
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
        let floor = plot_top + plot_height * Fixed::from_ratio(4, 5);
        let baseline = plot_top + plot_height * Fixed::from_ratio(2, 5);
        let muted = theme.resolve(TEXT_MUTED);
        let accent = theme.resolve(ACCENT);

        painter.line(
            Point::new(left, floor),
            Point::new(left + width, floor),
            theme.resolve(BORDER),
            Fixed::ONE,
            255,
        );
        for bar in 0..14 {
            let angle = state.phase() + Fixed::from_int(bar * 31);
            let height = Fixed::from_int(3)
                + Fixed::sin_deg(angle).abs() * plot_height * Fixed::from_ratio(3, 10);
            let x = left + width * Fixed::from_ratio(bar, 14);
            painter.fill(
                Rect::new(
                    x,
                    floor - height,
                    (width / Fixed::from_int(28)).max(Fixed::from_int(3)),
                    height,
                ),
                muted,
                Fixed::from_int(2),
                70,
            );
        }

        let mut previous = Point::new(left, baseline);
        for sample in 0..24 {
            let x = left + width * Fixed::from_ratio(sample, 23);
            let angle = state.phase() * Fixed::from_ratio(3, 2) + Fixed::from_int(sample * 23);
            let primary = Fixed::sin_deg(angle) * plot_height * Fixed::from_ratio(3, 10);
            let secondary =
                Fixed::sin_deg(angle * Fixed::from_ratio(5, 3)) * plot_height / Fixed::from_int(10);
            let point = Point::new(x, baseline - primary - secondary);
            if sample > 0 {
                painter.line(previous, point, accent, Fixed::from_ratio(3, 2), 255);
            }
            previous = point;
        }
    }
}

#[crate::view(
    component = OrbitInstrument,
    name = "OrbitInstrument",
    priority = 60
)]
pub(super) fn orbit_render(
    component: component!(),
    model: model!(component.model, watch(intensity, focused_node, phase)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let mut painter = DemoPainter::new(paint, transform);
    component.render(&mut painter, &rect, model, &theme);
}

#[crate::view(component = SignalMeter, name = "SignalMeter", priority = 60)]
pub(super) fn signal_render(
    component: component!(),
    model: model!(component.model, watch(intensity)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let mut painter = DemoPainter::new(paint, transform);
    component.render(&mut painter, &rect, model, &theme);
}

#[crate::view(component = ActivityPlot, name = "ActivityPlot", priority = 60)]
pub(super) fn activity_render(
    component: component!(),
    model: model!(component.model, watch(phase)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let mut painter = DemoPainter::new(paint, transform);
    component.render(&mut painter, &rect, model, &theme);
}
