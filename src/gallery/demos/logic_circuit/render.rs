use super::state::{CircuitModalSurface, CircuitSurface};
use super::style::{
    ACCENT, BACKGROUND, CARD, GREEN, INK, LINE, LIVE_STEP, LIVE_TOP, MUTED, OUTPUT_PAD, PAD_HEIGHT,
    PAD_LEFT, PAD_WIDTH, PANEL, SIDE, SIDE_LEFT, SIDE_WIDTH, SIGNAL_OFF, SIGNAL_ON, STAR_EMPTY,
    TRUTH_STEP, TRUTH_TOP, WORK,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::circuit::{
    BOARD, CircuitGate, CircuitModal, CircuitModel, CircuitPage, GATE_HALF_HEIGHT, GATE_HALF_WIDTH,
    INPUT_PIN_X, OUTPUT_PIN, PIN_SPREAD, SignalSource, input_y,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::path::Path;
use crate::types::Transform;

/// A five-pointed star of radius one, scaled by `paint_star`.
static STAR: Path = crate::path!(
    "M 0 -1 L 0.2245 -0.309 L 0.9511 -0.309 L 0.3633 0.118 L 0.5878 0.809 L 0 0.382 L -0.5878 0.809 L -0.3633 0.118 L -0.9511 -0.309 L -0.2245 -0.309 Z"
);

fn rect((x, y, w, h): (i32, i32, i32, i32)) -> Rect {
    Rect::new(x, y, w, h)
}

fn board_rect() -> Rect {
    let (x, y, w, h) = BOARD;
    Rect::new(i32::from(x), i32::from(y), i32::from(w), i32::from(h))
}

fn gate_with_position(
    gates: &[Option<CircuitGate>],
    positions: &[Option<(i16, i16)>],
    id: u8,
) -> Option<(CircuitGate, (i16, i16))> {
    gates.iter().zip(positions).find_map(|(gate, position)| {
        let gate = (*gate)?;
        (gate.id == id).then_some((gate, (*position)?))
    })
}

/// The pin of input `index`, right of its pad.
pub(super) fn input_pin(index: u8, count: u8) -> Point {
    Point::new(i32::from(INPUT_PIN_X), i32::from(input_y(index, count)))
}

pub(super) fn output_pin() -> Point {
    Point::new(i32::from(OUTPUT_PIN.0), i32::from(OUTPUT_PIN.1))
}

/// The top of the pad of input `index`, centred on its pin.
pub(super) fn pad_top(index: u8, count: u8) -> i32 {
    i32::from(input_y(index, count)) - PAD_HEIGHT / 2
}

pub(super) fn signal_position(
    input_count: u8,
    gates: &[Option<CircuitGate>],
    positions: &[Option<(i16, i16)>],
    source: SignalSource,
) -> Option<Point> {
    match source {
        SignalSource::None => None,
        SignalSource::Input(index) if index < input_count => Some(input_pin(index, input_count)),
        SignalSource::Gate(id) => gate_with_position(gates, positions, id)
            .map(|(_, (x, y))| Point::new(i32::from(x + GATE_HALF_WIDTH), i32::from(y))),
        _ => None,
    }
}

pub(super) fn gate_input_position(
    gates: &[Option<CircuitGate>],
    positions: &[Option<(i16, i16)>],
    gate_id: u8,
    pin: u8,
) -> Option<Point> {
    let (gate, (x, y)) = gate_with_position(gates, positions, gate_id)?;
    let dy = if gate.kind.inputs() == 1 {
        0
    } else if pin == 0 {
        -PIN_SPREAD
    } else {
        PIN_SPREAD
    };
    Some(Point::new(
        i32::from(x - GATE_HALF_WIDTH),
        i32::from(y + dy),
    ))
}

/// Wires from different sources turn on different lanes, so their vertical
/// runs never share a column; a fan-out of one source shares its lane.
fn lane_offset(source: SignalSource) -> i32 {
    match source {
        SignalSource::Input(index) => (i32::from(index) - 1) * 4,
        SignalSource::Gate(id) => (i32::from(id % 3) - 1) * 4,
        SignalSource::None => 0,
    }
}

/// The corners of a wire: out of the source, along its lane, into the pin.
fn wire_points(from: Point, to: Point, source: SignalSource) -> [Point; 4] {
    let middle = (from.x + to.x) / Fixed::from_int(2);
    let left = from.x.min(to.x) + Fixed::from_int(2);
    let right = from.x.max(to.x) - Fixed::from_int(2);
    let middle = if left < right {
        (middle + Fixed::from_int(lane_offset(source))).clamp(left, right)
    } else {
        middle
    };
    [
        from,
        Point {
            x: middle,
            y: from.y,
        },
        Point { x: middle, y: to.y },
        to,
    ]
}

fn segment_length(a: Point, b: Point) -> Fixed {
    (b.x - a.x).abs() + (b.y - a.y).abs()
}

/// The point `along` the axis-aligned segment from `a` to `b`.
fn segment_point(a: Point, b: Point, along: Fixed) -> Point {
    let length = segment_length(a, b);
    if length.is_zero() {
        return a;
    }
    Point {
        x: a.x + (b.x - a.x) * along / length,
        y: a.y + (b.y - a.y) * along / length,
    }
}

fn wire_style(on: bool) -> (Color, Fixed) {
    if on {
        (SIGNAL_ON, Fixed::from_int(2))
    } else {
        (SIGNAL_OFF, Fixed::ONE)
    }
}

/// Draws the part of the wire from `start` to `end`, measured along it.
fn paint_wire_span(
    painter: &mut PlayPainter<'_, '_>,
    points: &[Point; 4],
    start: Fixed,
    end: Fixed,
    on: bool,
) {
    let (color, width) = wire_style(on);
    let mut travelled = Fixed::ZERO;
    for pair in points.windows(2) {
        let length = segment_length(pair[0], pair[1]);
        let from = start.max(travelled);
        let to = end.min(travelled + length);
        if from < to {
            painter.line(
                segment_point(pair[0], pair[1], from - travelled),
                segment_point(pair[0], pair[1], to - travelled),
                color,
                width,
            );
        }
        travelled += length;
    }
}

/// A wire in the colour of what it carries. While a flipped input's wave
/// crosses it, the share behind the front shows the new value and the rest
/// the old one, with a spark at the front.
fn paint_wire(
    painter: &mut PlayPainter<'_, '_>,
    model: &CircuitModel,
    from: Point,
    to: Point,
    source: SignalSource,
) {
    let points = wire_points(from, to, source);
    let total = points.windows(2).fold(Fixed::ZERO, |sum, pair| {
        sum + segment_length(pair[0], pair[1])
    });
    let on = model.shown_value(source);
    let old = model.previous_value(source);
    match model.wire_front(source) {
        Some(share) if old != on => {
            let split = total * share;
            paint_wire_span(painter, &points, Fixed::ZERO, split, on);
            paint_wire_span(painter, &points, split, total, old);
            let mut travelled = Fixed::ZERO;
            for pair in points.windows(2) {
                let length = segment_length(pair[0], pair[1]);
                if split <= travelled + length {
                    let tip = segment_point(pair[0], pair[1], split - travelled);
                    painter.circle(tip, Fixed::from_int(3), if on { SIGNAL_ON } else { MUTED });
                    break;
                }
                travelled += length;
            }
        }
        _ => paint_wire_span(painter, &points, Fixed::ZERO, total, on),
    }
}

fn paint_pin(painter: &mut PlayPainter<'_, '_>, point: Point, fill: Color, ring: Color) {
    let radius = Fixed::from_ratio(7, 2);
    painter.circle(point, radius, fill);
    painter.border(
        Rect {
            x: point.x - radius,
            y: point.y - radius,
            w: Fixed::from_int(7),
            h: Fixed::from_int(7),
        },
        ring,
        Fixed::ONE,
        radius,
    );
}

fn paint_star(painter: &mut PlayPainter<'_, '_>, center: Point, radius: Fixed, color: Color) {
    if radius <= Fixed::ZERO {
        return;
    }
    painter
        .transformed(
            Transform::translate(center.x, center.y).compose(&Transform::scale(radius, radius)),
        )
        .fill_path(&STAR, color);
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 26), INK, Fixed::ZERO);
    painter.fill(board_rect(), WORK, Fixed::ZERO);
    painter.border(board_rect(), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(rect(SIDE), PANEL, Fixed::ZERO);
    painter.border(rect(SIDE), LINE, Fixed::ONE, Fixed::ZERO);
}

fn paint_wire_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    let input_count = model.input_count();
    let gates = model.gates();
    let positions = model.gate_positions();
    for x in (16..352).step_by(16) {
        for y in (40..292).step_by(16) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(1, 2),
                Color::rgb(196, 207, 202),
            );
        }
    }
    for gate in gates.iter().flatten().copied() {
        for (pin, source) in [gate.a, gate.b]
            .into_iter()
            .enumerate()
            .take(usize::from(gate.kind.inputs()))
        {
            if let Some(to) = gate_input_position(&gates, &positions, gate.id, pin as u8)
                && let Some(from) = signal_position(input_count, &gates, &positions, source)
            {
                paint_wire(painter, model, from, to, source);
            }
        }
    }
    if let Some(from) = signal_position(input_count, &gates, &positions, model.output_source()) {
        paint_wire(painter, model, from, output_pin(), model.output_source());
    }
    for index in 0..input_count {
        let pin = input_pin(index, input_count);
        painter.line(
            Point {
                x: Fixed::from_int(PAD_LEFT + PAD_WIDTH),
                y: pin.y,
            },
            pin,
            INK,
            Fixed::ONE,
        );
        paint_pin(
            painter,
            pin,
            if model.input(index) { ACCENT } else { PANEL },
            if model.pending() == SignalSource::input(index) {
                ACCENT
            } else {
                INK
            },
        );
    }
    for (gate, position) in gates.iter().zip(&positions) {
        let (Some(gate), Some((x, y))) = (*gate, *position) else {
            continue;
        };
        let (x, y) = (i32::from(x), i32::from(y));
        let (half_width, half_height) = (i32::from(GATE_HALF_WIDTH), i32::from(GATE_HALF_HEIGHT));
        let selected = gate.id == model.selected();
        let body = Rect::new(
            x - half_width,
            y - half_height,
            2 * half_width,
            2 * half_height,
        );
        painter.fill(body, PANEL, Fixed::ZERO);
        painter.border(
            body,
            if selected { ACCENT } else { MUTED },
            if selected {
                Fixed::from_int(2)
            } else {
                Fixed::ONE
            },
            Fixed::ZERO,
        );
        painter.fill(
            Rect::new(
                x - half_width + 1,
                y - half_height + 1,
                2 * half_width - 2,
                7,
            ),
            if selected { ACCENT } else { INK },
            Fixed::ZERO,
        );
        for pin in 0..gate.kind.inputs() {
            if let Some(point) = gate_input_position(&gates, &positions, gate.id, pin) {
                paint_pin(painter, point, PANEL, INK);
            }
        }
        let output = SignalSource::gate(gate.id);
        if let Some(point) = signal_position(input_count, &gates, &positions, output) {
            paint_pin(
                painter,
                point,
                if model.shown_value(output) {
                    ACCENT
                } else {
                    PANEL
                },
                if model.pending() == output {
                    ACCENT
                } else {
                    INK
                },
            );
        }
    }
    let shown = model.shown_output();
    painter.fill(
        rect(OUTPUT_PAD),
        if shown.value && shown.complete {
            ACCENT
        } else {
            INK
        },
        Fixed::ZERO,
    );
    paint_pin(painter, output_pin(), PANEL, INK);
    // The output the player picked waits for an input: ring it.
    if let Some(point) = signal_position(input_count, &gates, &positions, model.pending()) {
        painter.border(
            Rect {
                x: point.x - Fixed::from_int(7),
                y: point.y - Fixed::from_int(7),
                w: Fixed::from_int(14),
                h: Fixed::from_int(14),
            },
            ACCENT,
            Fixed::from_ratio(3, 2),
            Fixed::from_int(7),
        );
    }
    paint_live_table(painter, model);
}

/// Tints of the side table: a passing row turns green, a row that failed
/// the last check turns red; then the bar of passing rows.
fn paint_live_table(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    let rows = 1_u8 << model.input_count();
    let checked = model.verify_result().is_some();
    for index in 0..rows {
        let Some(row) = model.truth_row(index) else {
            continue;
        };
        let tint = if row.passes() {
            Color::rgb(226, 236, 218)
        } else if checked {
            Color::rgb(246, 225, 216)
        } else {
            continue;
        };
        painter.fill(
            Rect::new(
                SIDE_LEFT - 3,
                LIVE_TOP + i32::from(index) * LIVE_STEP,
                SIDE_WIDTH + 6,
                LIVE_STEP - 1,
            ),
            tint,
            Fixed::ZERO,
        );
    }
    let passed = (0..rows)
        .filter(|row| model.truth_row(*row).is_some_and(|row| row.passes()))
        .count() as i32;
    painter.fill(Rect::new(SIDE_LEFT, 204, SIDE_WIDTH, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(SIDE_LEFT, 204, SIDE_WIDTH * passed / i32::from(rows), 4),
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
    for index in 0..rows {
        let fails = model.truth_row(index).is_some_and(|row| !row.passes());
        if fails || index % 2 == 0 {
            painter.fill(
                Rect::new(
                    10,
                    TRUTH_TOP + i32::from(index) * TRUTH_STEP,
                    338,
                    TRUTH_STEP,
                ),
                if fails {
                    Color::rgb(246, 225, 216)
                } else {
                    Color::rgb(237, 240, 233)
                },
                Fixed::ZERO,
            );
        }
    }
}

/// The high line of channel `channel` of `channels` on the trace page.
pub(super) fn trace_base(channel: usize, channels: usize) -> i32 {
    54 + channel as i32 * (216 / channels as i32)
}

fn paint_trace_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    painter.fill(Rect::new(6, 32, 346, 260), INK, Fixed::ZERO);
    let channels = usize::from(model.input_count()) + 1;
    for channel in 0..channels {
        let logical = if channel == channels - 1 { 3 } else { channel };
        let base = trace_base(channel, channels);
        painter.line(
            Point::new(36, base + 22),
            Point::new(344, base + 22),
            Color::rgb(70, 87, 99),
            Fixed::ONE,
        );
        let color = if logical == 3 {
            ACCENT
        } else {
            Color::rgb(182, 204, 211)
        };
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
            let x = 40 + sample_index as i32 * 9;
            let y = if value { base } else { base + 19 };
            if let Some((last_x, last_y)) = previous {
                painter.line(
                    Point::new(last_x, last_y),
                    Point::new(x, last_y),
                    color,
                    Fixed::from_ratio(3, 2),
                );
                painter.line(
                    Point::new(x, last_y),
                    Point::new(x, y),
                    color,
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

#[crate::view(component = CircuitSurface, name = "CircuitSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_circuit_surface(&mut painter, model);
}

/// The card after a pass: a title band and three stars, the earned ones
/// growing in one after another.
fn paint_result(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    let (x, y, w, h) = CARD;
    painter.fill(Rect::new(x, y, w, h), PANEL, Fixed::ZERO);
    painter.border(Rect::new(x, y, w, h), INK, Fixed::from_int(2), Fixed::ZERO);
    painter.fill(Rect::new(x + 3, y + 3, w - 6, 26), INK, Fixed::ZERO);
    let earned = model.verify_result().map_or(0, |result| result.stars);
    for index in 0..3 {
        let center = Point::new(x + w / 2 + (index - 1) * 46, y + 60);
        paint_star(painter, center, Fixed::from_int(17), STAR_EMPTY);
        if index < i32::from(earned) {
            paint_star(
                painter,
                center,
                Fixed::from_int(17) * model.star_scale(index as usize),
                ACCENT,
            );
        }
    }
}

#[crate::view(
    component = CircuitModalSurface,
    name = "CircuitModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    if model.modal() == CircuitModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 200),
        Fixed::ZERO,
    );
    if model.modal() == CircuitModal::Result {
        paint_result(&mut painter, model);
        return;
    }
    painter.fill(Rect::new(25, 46, 430, 236), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 46, 430, 236),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 50, 422, 30), INK, Fixed::ZERO);
}
