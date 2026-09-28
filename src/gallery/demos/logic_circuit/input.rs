use super::render::{gate_input_position, signal_position};
use super::state::CircuitNodes;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::circuit::{
    CircuitModal, CircuitModel, CircuitPage, GateKind, SignalSource,
};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Entity, Fixed, Point, World};
use crate::ui::ComputedRect;

fn local_point(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = world.get::<ComputedRect>(entity)?.0;
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

fn tap_surface(model: &mut CircuitModel, point: Point) -> ChangeSet {
    if model.page() != CircuitPage::Wire || model.modal() != CircuitModal::None {
        return ChangeSet::NONE;
    }
    for index in 0..model.input_count() {
        let y = 100 + i32::from(index) * 54;
        if point.x >= Fixed::from_int(17)
            && point.x <= Fixed::from_int(47)
            && point.y >= Fixed::from_int(y - 13)
            && point.y <= Fixed::from_int(y + 13)
        {
            return model.toggle_input(index);
        }
        if within(point, Point::new(54, y), 9) {
            return model.select_source(SignalSource::input(index));
        }
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        if let Some(output) = signal_position(model, SignalSource::gate(gate.id))
            && within(point, output, 9)
        {
            return model.select_source(SignalSource::gate(gate.id));
        }
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(input) = gate_input_position(model, gate.id, pin)
                && within(point, input, 9)
            {
                return model
                    .connect_pending_to(Some(gate.id), pin)
                    .unwrap_or(ChangeSet::VISUAL);
            }
        }
    }
    if within(point, Point::new(288, 153), 12) {
        return model
            .connect_pending_to(None, 0)
            .unwrap_or(ChangeSet::VISUAL);
    }
    if let Some(gate) = hit_gate(model, point) {
        return model.select_gate(gate);
    }
    model.clear_pending()
}

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let (x, y) = match event {
        GestureEvent::Tap { x, y, .. }
        | GestureEvent::DragStart { x, y, .. }
        | GestureEvent::DragMove { x, y, .. }
        | GestureEvent::DragEnd { x, y, .. }
        | GestureEvent::DragCancel { x, y, .. } => (*x, *y),
        _ => return false,
    };
    let Some(point) = local_point(world, entity, x, y) else {
        return false;
    };
    CircuitNodes::update(world, |model| match event {
        GestureEvent::Tap { .. } => tap_surface(model, point),
        GestureEvent::DragStart { .. } => hit_gate(model, point).map_or(ChangeSet::NONE, |gate| {
            model.begin_drag_at(gate, point.x.to_int() as i16, point.y.to_int() as i16)
        }),
        GestureEvent::DragMove { .. } => {
            model.move_drag(point.x.to_int() as i16, point.y.to_int() as i16)
        }
        GestureEvent::DragEnd { .. } => model.end_drag(false).unwrap_or(ChangeSet::VISUAL),
        GestureEvent::DragCancel { .. } => model.end_drag(true).unwrap_or(ChangeSet::VISUAL),
        _ => ChangeSet::NONE,
    });
    true
}

pub(super) fn footer_action(world: &mut World, index: usize) {
    let page = world
        .resource::<CircuitModel>()
        .map_or(CircuitPage::Wire, CircuitModel::page);
    if page == CircuitPage::Trace {
        match index {
            0 => CircuitNodes::update(world, CircuitModel::step_trace),
            1 => CircuitNodes::update(world, CircuitModel::toggle_scanning),
            2 => CircuitNodes::update(world, CircuitModel::clear_trace),
            3 => CircuitNodes::update(world, CircuitModel::verify),
            _ => CircuitNodes::update(world, |model| model.set_page(CircuitPage::Wire)),
        }
    } else {
        match index {
            0 => CircuitNodes::update(world, |model| {
                model.open_modal(CircuitModal::GateTypes { adding: true })
            }),
            1 => CircuitNodes::update(world, |model| {
                model.open_modal(CircuitModal::GateTypes { adding: false })
            }),
            2 => CircuitNodes::update(world, CircuitModel::toggle_disconnecting),
            3 => CircuitNodes::update(world, CircuitModel::undo),
            _ => CircuitNodes::update(world, CircuitModel::verify),
        }
    }
}

pub(super) fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<CircuitModel>()
        .map_or(CircuitModal::None, CircuitModel::modal);
    match modal {
        CircuitModal::Tasks => CircuitNodes::update(world, |model| {
            model.load_task((index / 2) as u8, index % 2 == 1)
        }),
        CircuitModal::GateTypes { adding } if index < GateKind::ALL.len() => {
            if adding {
                CircuitNodes::result(world, |model| model.add_gate(GateKind::ALL[index]));
            } else {
                CircuitNodes::result(world, |model| model.set_selected_kind(GateKind::ALL[index]));
            }
            CircuitNodes::update(world, CircuitModel::close_modal);
        }
        CircuitModal::GateTypes { adding: false } if index == 5 => {
            CircuitNodes::result(world, CircuitModel::remove_selected);
            CircuitNodes::update(world, CircuitModel::close_modal);
        }
        CircuitModal::Help => CircuitNodes::update(world, CircuitModel::close_modal),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn circuit_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    CircuitNodes::update(world, |model| model.advance_ms(elapsed));
}
