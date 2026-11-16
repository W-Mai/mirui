use super::render::{gate_input_position, input_pin, output_pin, signal_position};
use super::state::CircuitSurface;
use super::style::{OUTPUT_PAD, PAD_HEIGHT, PAD_LEFT, PAD_WIDTH};
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::circuit::{
    CircuitGate, CircuitModal, CircuitModel, CircuitPage, GATE_HALF_HEIGHT, GATE_HALF_WIDTH,
    SignalSource,
};
use crate::prelude::*;

/// Pins take taps this far from their centre. The two inputs of a gate sit
/// 20 px apart, so theirs reach less far.
const SOURCE_REACH: i32 = 11;
const INPUT_REACH: i32 = 9;

fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Point> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

fn within(point: Point, center: Point, radius: i32) -> bool {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    dx * dx + dy * dy <= Fixed::from_int(radius * radius)
}

fn inside(point: Point, (x, y, w, h): (i32, i32, i32, i32)) -> bool {
    point.x >= Fixed::from_int(x)
        && point.x <= Fixed::from_int(x + w)
        && point.y >= Fixed::from_int(y)
        && point.y <= Fixed::from_int(y + h)
}

fn hit_gate(
    gates: &[Option<CircuitGate>],
    positions: &[Option<(i16, i16)>],
    point: Point,
) -> Option<u8> {
    gates.iter().zip(positions).find_map(|(gate, position)| {
        let gate = (*gate)?;
        let (x, y) = (*position)?;
        let left = i32::from(x - GATE_HALF_WIDTH);
        let top = i32::from(y - GATE_HALF_HEIGHT);
        inside(
            point,
            (
                left,
                top,
                2 * i32::from(GATE_HALF_WIDTH),
                2 * i32::from(GATE_HALF_HEIGHT),
            ),
        )
        .then_some(gate.id)
    })
}

#[derive(Clone, Copy)]
enum SurfaceAction {
    ToggleInput(u8),
    SelectSource(SignalSource),
    Connect { gate: Option<u8>, pin: u8 },
    TapGate(u8),
    ClearPending,
}

fn tap_action(surface: &CircuitSurface, point: Point) -> Option<SurfaceAction> {
    if surface.model.page() != CircuitPage::Wire || surface.model.modal() != CircuitModal::None {
        return None;
    }
    let input_count = surface.model.input_count();
    let gates = surface.model.gates();
    let positions = surface.model.gate_positions();
    for index in 0..input_count {
        let pin = input_pin(index, input_count);
        let top = pin.y.to_int() - PAD_HEIGHT / 2;
        if inside(point, (PAD_LEFT, top, PAD_WIDTH, PAD_HEIGHT)) {
            return Some(SurfaceAction::ToggleInput(index));
        }
        if within(point, pin, SOURCE_REACH) {
            return Some(SurfaceAction::SelectSource(SignalSource::input(index)));
        }
    }
    for gate in gates.iter().flatten().copied() {
        if let Some(output) =
            signal_position(input_count, &gates, &positions, SignalSource::gate(gate.id))
            && within(point, output, SOURCE_REACH)
        {
            return Some(SurfaceAction::SelectSource(SignalSource::gate(gate.id)));
        }
        for pin in 0..gate.kind.inputs() {
            if let Some(input) = gate_input_position(&gates, &positions, gate.id, pin)
                && within(point, input, INPUT_REACH)
            {
                return Some(SurfaceAction::Connect {
                    gate: Some(gate.id),
                    pin,
                });
            }
        }
    }
    if within(point, output_pin(), SOURCE_REACH) || inside(point, OUTPUT_PAD) {
        return Some(SurfaceAction::Connect { gate: None, pin: 0 });
    }
    Some(
        hit_gate(&gates, &positions, point)
            .map_or(SurfaceAction::ClearPending, SurfaceAction::TapGate),
    )
}

fn apply_tap(surface: &CircuitSurface, action: SurfaceAction) {
    match action {
        SurfaceAction::ToggleInput(index) => {
            surface.model.toggle_input(index);
        }
        SurfaceAction::SelectSource(source) => {
            surface.model.select_source(source);
        }
        SurfaceAction::Connect { gate, pin } => {
            surface.model.connect_from_tap(gate, pin);
        }
        SurfaceAction::TapGate(id) => {
            surface.model.tap_gate(id);
        }
        SurfaceAction::ClearPending => {
            surface.model.clear_pending();
        }
    }
}

pub(super) fn surface_tap(surface: &CircuitSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    if let Some(action) = tap_action(surface, point) {
        apply_tap(surface, action);
    }
    true
}

pub(super) fn surface_drag_start(surface: &CircuitSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    if surface.model.page() != CircuitPage::Wire || surface.model.modal() != CircuitModal::None {
        return true;
    }
    let gates = surface.model.gates();
    let positions = surface.model.gate_positions();
    if let Some(gate) = hit_gate(&gates, &positions, point) {
        surface
            .model
            .begin_drag_at(gate, point.x.to_int() as i16, point.y.to_int() as i16);
    }
    true
}

pub(super) fn surface_drag_move(surface: &CircuitSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    surface
        .model
        .move_drag(point.x.to_int() as i16, point.y.to_int() as i16);
    true
}

pub(super) fn surface_drag_end(
    surface: &CircuitSurface,
    rect: Rect,
    x: Fixed,
    y: Fixed,
    cancelled: bool,
) -> bool {
    if local_point(rect, x, y).is_none() {
        return false;
    }
    if cancelled || surface.model.end_drag().is_err() {
        surface.model.cancel_drag();
    }
    true
}

/// Desktop keys: 1 2 3 flip the inputs, Z undoes, Delete removes the
/// selected gate, Enter checks or moves on, Esc closes a dialog or drops
/// a picked output.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(CircuitModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    let modal = model.modal();
    match input.physical {
        Some(PhysicalKey::Escape) if modal != CircuitModal::None => {
            model.close_modal();
        }
        Some(PhysicalKey::Escape) => {
            model.clear_pending();
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) if modal == CircuitModal::Result => {
            model.choose_modal_option(0);
        }
        _ if modal != CircuitModal::None => return false,
        Some(PhysicalKey::Digit1) => {
            model.toggle_input(0);
        }
        Some(PhysicalKey::Digit2) => {
            model.toggle_input(1);
        }
        Some(PhysicalKey::Digit3) => {
            model.toggle_input(2);
        }
        Some(PhysicalKey::KeyZ) => {
            model.undo();
        }
        Some(PhysicalKey::Delete | PhysicalKey::Backspace) => {
            model.remove_from_tap();
        }
        Some(PhysicalKey::Enter | PhysicalKey::NumpadEnter) => {
            model.verify_or_next();
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = Animation)]
pub(super) fn circuit_tick_system(model: model!(CircuitModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
