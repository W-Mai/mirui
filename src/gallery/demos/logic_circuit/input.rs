use super::render::{gate_input_position, signal_position};
use super::state::CircuitSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::circuit::{
    CircuitGate, CircuitModal, CircuitModel, CircuitPage, GateKind, SignalSource,
};
use crate::prelude::{Fixed, Point, Rect};

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

fn hit_gate(
    gates: &[Option<CircuitGate>],
    positions: &[Option<(i16, i16)>],
    point: Point,
) -> Option<u8> {
    gates.iter().zip(positions).find_map(|(gate, position)| {
        let gate = (*gate)?;
        let (x, y) = (*position)?;
        ((point.x - Fixed::from_int(i32::from(x))).abs() <= Fixed::from_int(30)
            && (point.y - Fixed::from_int(i32::from(y))).abs() <= Fixed::from_int(18))
        .then_some(gate.id)
    })
}

#[derive(Clone, Copy)]
enum SurfaceAction {
    ToggleInput(u8),
    SelectSource(SignalSource),
    Connect { gate: Option<u8>, pin: u8 },
    SelectGate(u8),
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
        let y = 100 + i32::from(index) * 54;
        if point.x >= Fixed::from_int(17)
            && point.x <= Fixed::from_int(47)
            && point.y >= Fixed::from_int(y - 13)
            && point.y <= Fixed::from_int(y + 13)
        {
            return Some(SurfaceAction::ToggleInput(index));
        }
        if within(point, Point::new(54, y), 9) {
            return Some(SurfaceAction::SelectSource(SignalSource::input(index)));
        }
    }
    for gate in gates.iter().flatten().copied() {
        if let Some(output) =
            signal_position(input_count, &gates, &positions, SignalSource::gate(gate.id))
            && within(point, output, 9)
        {
            return Some(SurfaceAction::SelectSource(SignalSource::gate(gate.id)));
        }
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(input) = gate_input_position(&gates, &positions, gate.id, pin)
                && within(point, input, 9)
            {
                return Some(SurfaceAction::Connect {
                    gate: Some(gate.id),
                    pin,
                });
            }
        }
    }
    if within(point, Point::new(288, 153), 12) {
        return Some(SurfaceAction::Connect { gate: None, pin: 0 });
    }
    Some(
        hit_gate(&gates, &positions, point)
            .map_or(SurfaceAction::ClearPending, SurfaceAction::SelectGate),
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
            let _ = surface.model.connect_pending_to(gate, pin);
        }
        SurfaceAction::SelectGate(id) => {
            surface.model.select_gate(id);
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

#[mirui_macros::system(order = Animation)]
pub(super) fn circuit_tick_system(model: model!(CircuitModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
