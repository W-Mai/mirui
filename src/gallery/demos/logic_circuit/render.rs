use super::state::{CircuitModalSurface, CircuitSurface};
use super::style::{
    ACCENT, BACKGROUND, GREEN, INK, LINE, MUTED, PANEL, RED, SIGNAL_OFF, SIGNAL_ON, WORK,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::circuit::{
    CircuitModal, CircuitModel, CircuitPage, GateKind, SignalSource,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

pub(super) fn signal_position(model: &CircuitModel, source: SignalSource) -> Option<Point> {
    match source {
        SignalSource::None => None,
        SignalSource::Input(index) if index < model.input_count() => {
            Some(Point::new(54, 100 + i32::from(index) * 54))
        }
        SignalSource::Gate(id) => model
            .visual_gate_position(id)
            .map(|(x, y)| Point::new(i32::from(x) + 31, i32::from(y))),
        _ => None,
    }
}

pub(super) fn gate_input_position(model: &CircuitModel, gate_id: u8, pin: u8) -> Option<Point> {
    let gate = model.gate_by_id(gate_id)?;
    let (x, y) = model.visual_gate_position(gate_id)?;
    let dy = if gate.kind == GateKind::Not {
        0
    } else if pin == 0 {
        -9
    } else {
        9
    };
    Some(Point::new(i32::from(x) - 31, i32::from(y) + dy))
}

fn draw_wire(painter: &mut PlayPainter<'_, '_>, from: Point, to: Point, on: bool) {
    let color = if on { SIGNAL_ON } else { SIGNAL_OFF };
    let width = if on {
        Fixed::from_ratio(3, 2)
    } else {
        Fixed::ONE
    };
    let middle = (from.x + to.x) / Fixed::from_int(2);
    painter.line(
        from,
        Point {
            x: middle,
            y: from.y,
        },
        color,
        width,
    );
    painter.line(
        Point {
            x: middle,
            y: from.y,
        },
        Point { x: middle, y: to.y },
        color,
        width,
    );
    painter.line(Point { x: middle, y: to.y }, to, color, width);
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 226),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
    painter.fill(Rect::new(11, 65, 305, 188), WORK, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 305, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(324, 66, 145, 186), PANEL, Fixed::ZERO);
    painter.border(Rect::new(324, 66, 145, 186), LINE, Fixed::ONE, Fixed::ZERO);
}

fn paint_wire_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    for x in (20..313).step_by(12) {
        for y in (74..252).step_by(12) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(1, 2),
                Color::rgb(196, 207, 202),
            );
        }
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        if let Some(to) = gate_input_position(model, gate.id, 0)
            && let Some(from) = signal_position(model, gate.a)
        {
            draw_wire(painter, from, to, model.source_value(gate.a));
        }
        if gate.kind != GateKind::Not
            && let Some(to) = gate_input_position(model, gate.id, 1)
            && let Some(from) = signal_position(model, gate.b)
        {
            draw_wire(painter, from, to, model.source_value(gate.b));
        }
    }
    if let Some(from) = signal_position(model, model.output_source()) {
        draw_wire(
            painter,
            from,
            Point::new(288, 153),
            model.source_value(model.output_source()),
        );
    }
    for index in 0..usize::from(model.input_count()) {
        let y = 100 + index as i32 * 54;
        let on = model.input(index as u8);
        painter.fill(
            Rect::new(17, y - 13, 30, 26),
            if on { Color::rgb(255, 238, 228) } else { PANEL },
            Fixed::ZERO,
        );
        painter.border(
            Rect::new(17, y - 13, 30, 26),
            if on { ACCENT } else { LINE },
            Fixed::ONE,
            Fixed::ZERO,
        );
        painter.line(Point::new(47, y), Point::new(54, y), INK, Fixed::ONE);
        painter.circle(Point::new(54, y), Fixed::from_int(3), PANEL);
        painter.border(
            Rect::new(51, y - 3, 6, 6),
            if on { ACCENT } else { INK },
            Fixed::ONE,
            Fixed::from_int(3),
        );
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        let Some((x, y)) = model.visual_gate_position(gate.id) else {
            continue;
        };
        let selected = gate.id == model.selected();
        painter.fill(
            Rect::new(i32::from(x) - 30, i32::from(y) - 18, 60, 36),
            PANEL,
            Fixed::ZERO,
        );
        painter.border(
            Rect::new(i32::from(x) - 30, i32::from(y) - 18, 60, 36),
            if selected { ACCENT } else { MUTED },
            if selected {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::ONE
            },
            Fixed::ZERO,
        );
        painter.fill(
            Rect::new(i32::from(x) - 29, i32::from(y) - 17, 58, 8),
            if selected { ACCENT } else { INK },
            Fixed::ZERO,
        );
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(point) = gate_input_position(model, gate.id, pin) {
                painter.circle(point, Fixed::from_int(3), PANEL);
                painter.border(
                    Rect {
                        x: point.x - Fixed::from_int(3),
                        y: point.y - Fixed::from_int(3),
                        w: Fixed::from_int(6),
                        h: Fixed::from_int(6),
                    },
                    if model.disconnecting() { RED } else { INK },
                    Fixed::ONE,
                    Fixed::from_int(3),
                );
            }
        }
        if let Some(point) = signal_position(model, SignalSource::gate(gate.id)) {
            painter.circle(
                point,
                Fixed::from_int(3),
                if model.source_value(SignalSource::gate(gate.id)) {
                    ACCENT
                } else {
                    PANEL
                },
            );
            painter.border(
                Rect {
                    x: point.x - Fixed::from_int(3),
                    y: point.y - Fixed::from_int(3),
                    w: Fixed::from_int(6),
                    h: Fixed::from_int(6),
                },
                if model.pending() == SignalSource::gate(gate.id) {
                    ACCENT
                } else {
                    INK
                },
                Fixed::ONE,
                Fixed::from_int(3),
            );
        }
    }
    let evaluation = model.evaluation();
    painter.fill(
        Rect::new(288, 139, 24, 28),
        if evaluation.value { ACCENT } else { INK },
        Fixed::ZERO,
    );
    painter.circle(Point::new(288, 153), Fixed::from_int(3), PANEL);
    painter.border(
        Rect::new(285, 150, 6, 6),
        INK,
        Fixed::ONE,
        Fixed::from_int(3),
    );
    let rows = 1_u8 << model.input_count();
    let passed = (0..rows)
        .filter(|row| model.truth_row(*row).is_some_and(|row| row.passes()))
        .count() as i32;
    painter.fill(Rect::new(334, 239, 123, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(334, 239, 123 * passed / i32::from(rows), 4),
        if passed == i32::from(rows) {
            GREEN
        } else {
            ACCENT
        },
        Fixed::ZERO,
    );
}

fn paint_truth_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    let rows = 1_u8 << model.input_count();
    let step = if rows == 8 { 16 } else { 30 };
    for index in 0..rows {
        if index % 2 == 0 {
            painter.fill(
                Rect::new(18, 112 + i32::from(index) * step, 276, step),
                Color::rgb(237, 240, 233),
                Fixed::ZERO,
            );
        }
    }
}

fn paint_trace_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    painter.fill(Rect::new(13, 67, 286, 186), INK, Fixed::ZERO);
    let channels = usize::from(model.input_count()) + 1;
    for channel in 0..channels {
        let logical = if channel == channels - 1 { 3 } else { channel };
        let base = 84 + channel as i32 * (150 / channels as i32);
        painter.line(
            Point::new(43, base + 20),
            Point::new(289, base + 20),
            Color::rgb(70, 87, 99),
            Fixed::ONE,
        );
        let mut previous = None;
        for sample_index in 0..usize::from(model.trace_len()) {
            let Some(sample) = model.trace(sample_index) else {
                continue;
            };
            let value = if logical == 3 {
                sample.output
            } else {
                sample.inputs & (1 << logical) != 0
            };
            let x = 47 + sample_index as i32 * 7;
            let y = if value { base } else { base + 17 };
            if let Some((last_x, last_y)) = previous {
                painter.line(
                    Point::new(last_x, last_y),
                    Point::new(x, last_y),
                    if logical == 3 {
                        ACCENT
                    } else {
                        Color::rgb(182, 204, 211)
                    },
                    Fixed::from_ratio(3, 2),
                );
                painter.line(
                    Point::new(x, last_y),
                    Point::new(x, y),
                    if logical == 3 {
                        ACCENT
                    } else {
                        Color::rgb(182, 204, 211)
                    },
                    Fixed::from_ratio(3, 2),
                );
            }
            previous = Some((x, y));
        }
    }
}

fn paint_circuit_surface(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    paint_shell(painter);
    match model.page() {
        CircuitPage::Wire => paint_wire_page(painter, model),
        CircuitPage::Truth => paint_truth_page(painter, model),
        CircuitPage::Trace => paint_trace_page(painter, model),
    }
}

#[crate::view(
    component = CircuitSurface,
    read(model),
    watch(model.visual_revision()),
    name = "CircuitSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &CircuitModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_circuit_surface(&mut painter, model);
}

#[crate::view(
    component = CircuitModalSurface,
    read(model),
    watch(model.modal()),
    name = "CircuitModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &CircuitModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == CircuitModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(25, 62, 430, 209), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 62, 430, 209),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 66, 422, 32), INK, Fixed::ZERO);
}

pub(super) fn surface_view() -> View {
    surface_render::view()
}

pub(super) fn modal_view() -> View {
    modal_render::view()
}
