use super::render::{gate_input_position, signal_position};
use super::state::CircuitSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::circuit::{
    CircuitModal, CircuitModel, CircuitModelHandle, CircuitPage, GateKind, SignalSource,
};
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Fixed, Point};
use crate::ui::ComputedRect;

fn local_point(ctx: &HandlerCtx<'_, GestureEvent>, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = ctx.component::<ComputedRect>(ctx.entity)?.0;
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

fn hit_gate(model: &CircuitModel, point: Point) -> Option<u8> {
    (0..usize::from(model.gate_len())).find_map(|index| {
        let gate = model.gate(index)?;
        let (x, y) = model.visual_gate_position(gate.id)?;
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

fn tap_action(model: &CircuitModel, point: Point) -> Option<SurfaceAction> {
    if model.page() != CircuitPage::Wire || model.modal() != CircuitModal::None {
        return None;
    }
    for index in 0..model.input_count() {
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
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        if let Some(output) = signal_position(model, SignalSource::gate(gate.id))
            && within(point, output, 9)
        {
            return Some(SurfaceAction::SelectSource(SignalSource::gate(gate.id)));
        }
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(input) = gate_input_position(model, gate.id, pin)
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
    Some(hit_gate(model, point).map_or(SurfaceAction::ClearPending, SurfaceAction::SelectGate))
}

fn apply_tap(model: &CircuitModelHandle, action: SurfaceAction) {
    match action {
        SurfaceAction::ToggleInput(index) => {
            model.toggle_input(index);
        }
        SurfaceAction::SelectSource(source) => {
            model.select_source(source);
        }
        SurfaceAction::Connect { gate, pin } => {
            let _ = model.connect_pending_to(gate, pin);
        }
        SurfaceAction::SelectGate(id) => {
            model.select_gate(id);
        }
        SurfaceAction::ClearPending => {
            model.clear_pending();
        }
    }
}

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let Some(model) = ctx
        .component::<CircuitSurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    let (x, y) = match ctx.event {
        GestureEvent::Tap { x, y, .. }
        | GestureEvent::DragStart { x, y, .. }
        | GestureEvent::DragMove { x, y, .. }
        | GestureEvent::DragEnd { x, y, .. }
        | GestureEvent::DragCancel { x, y, .. } => (*x, *y),
        _ => return false,
    };
    let Some(point) = local_point(ctx, x, y) else {
        return false;
    };
    match ctx.event {
        GestureEvent::Tap { .. } => {
            if let Some(action) =
                crate::core::model::ModelHandle::read(&model, |model| tap_action(model, point))
            {
                apply_tap(&model, action);
            }
        }
        GestureEvent::DragStart { .. } => {
            if let Some(gate) =
                crate::core::model::ModelHandle::read(&model, |model| hit_gate(model, point))
            {
                model.begin_drag_at(gate, point.x.to_int() as i16, point.y.to_int() as i16);
            }
        }
        GestureEvent::DragMove { .. } => {
            model.move_drag(point.x.to_int() as i16, point.y.to_int() as i16);
        }
        GestureEvent::DragEnd { .. } => {
            if model.end_drag().is_err() {
                model.cancel_drag();
            }
        }
        GestureEvent::DragCancel { .. } => {
            model.cancel_drag();
        }
        _ => unreachable!(),
    }
    true
}

pub(super) fn footer_action(model: &CircuitModelHandle, index: usize) {
    if model.page() == CircuitPage::Trace {
        match index {
            0 => model.step_trace(),
            1 => model.toggle_scanning(),
            2 => model.clear_trace(),
            3 => model.verify(),
            _ => model.set_page(CircuitPage::Wire),
        };
    } else {
        match index {
            0 => model.open_modal(CircuitModal::GateTypes { adding: true }),
            1 => model.open_modal(CircuitModal::GateTypes { adding: false }),
            2 => model.toggle_disconnecting(),
            3 => model.undo(),
            _ => model.verify(),
        };
    }
}

pub(super) fn modal_action(model: &CircuitModelHandle, index: usize) {
    match model.modal() {
        CircuitModal::Tasks => {
            model.load_task((index / 2) as u8, index % 2 == 1);
        }
        CircuitModal::GateTypes { adding } if index < GateKind::ALL.len() => {
            if adding {
                let _ = model.add_gate(GateKind::ALL[index]);
            } else {
                let _ = model.set_selected_kind(GateKind::ALL[index]);
            }
            model.close_modal();
        }
        CircuitModal::GateTypes { adding: false } if index == 5 => {
            let _ = model.remove_selected();
            model.close_modal();
        }
        CircuitModal::Help => {
            model.close_modal();
        }
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn circuit_tick_system(model: &CircuitModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}
