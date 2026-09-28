use super::setup_app;
use super::state::CircuitSurface;
use crate::core::model::ModelHandle;
use crate::gallery::play::circuit::{CircuitError, CircuitModel, CircuitModelHandle, CircuitPage};
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{App, Fixed, Rect, World};
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{ComputedRect, Hidden};

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn model_handle(world: &World) -> CircuitModelHandle {
    let surface = world.find_by_id("circuit_surface").unwrap();
    world.get::<CircuitSurface>(surface).unwrap().model.clone()
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&CircuitModel) -> R) -> R {
    ModelHandle::read(&model_handle(world), inspect)
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert_eq!(text.resolve(world).as_ref(), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

#[test]
fn composition_binds_one_model_to_surface_and_modal() {
    let world = fixture();
    assert!(world.find_by_id("circuit_surface").is_some());
    assert!(world.find_by_id("circuit_footer_4").is_some());
    assert!(world.find_by_id("circuit_task").is_some());
    assert_eq!(world.query::<CircuitSurface>().collect().len(), 1);
}

#[test]
fn gate_drag_cancel_uses_the_bound_model() {
    let mut world = fixture();
    let surface = world.find_by_id("circuit_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let before = with_model(&world, |model| model.gate_by_id(1).unwrap());
    for event in [
        GestureEvent::DragStart {
            x: Fixed::from_int(145),
            y: Fixed::from_int(150),
            target: surface,
        },
        GestureEvent::DragMove {
            x: Fixed::from_int(180),
            y: Fixed::from_int(210),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface,
        },
        GestureEvent::DragCancel {
            x: Fixed::from_int(180),
            y: Fixed::from_int(210),
            target: surface,
        },
    ] {
        assert_eq!(
            GestureHandler::trigger(&mut world, surface, &event),
            Some(true)
        );
    }
    assert_eq!(
        with_model(&world, |model| model.gate_by_id(1).unwrap()),
        before
    );
}

#[test]
fn rejected_drag_is_an_uncommitted_model_transaction() {
    let mut world = fixture();
    let surface = world.find_by_id("circuit_surface").unwrap();
    let model = model_handle(&world);
    model.load_task(1, true);
    let (from, to) = with_model(&world, |model| {
        let from = model.gate_by_id(1).unwrap();
        let to = model.gate_by_id(2).unwrap();
        ((from.x, from.y), (to.x, to.y))
    });
    model.begin_drag_at(1, from.0, from.1);
    model.move_drag(to.0, to.1);
    flush(&mut world);
    world.remove::<VisualDirty>(surface);
    let revision = model.visual_revision();
    let before = with_model(&world, CircuitModel::gate_positions);

    assert_eq!(model.end_drag(), Err(CircuitError::Overlap));
    assert_eq!(model.visual_revision(), revision);
    assert_eq!(with_model(&world, CircuitModel::gate_positions), before);
    flush(&mut world);
    assert!(!world.has::<VisualDirty>(surface));

    model.cancel_drag();
}

#[test]
fn observed_state_drives_bounded_text_and_page_visibility() {
    let mut world = fixture();
    let model = model_handle(&world);

    assert_text(&world, "circuit_gate_count", "NET / 1:6 GATES");
    assert_text(&world, "circuit_task", "任务 01 · 不同才亮");
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));

    model.set_page(CircuitPage::Trace);
    model.step_trace();
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_trace_page").unwrap()));
    assert_text(&world, "circuit_trace_count", "1 / 32");

    for id in [
        "circuit_gate_count",
        "circuit_task",
        "circuit_input_0",
        "circuit_gate_0",
        "circuit_output",
        "circuit_live_0",
        "circuit_truth_0",
        "circuit_trace_count",
        "circuit_modal_title",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.text_capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}
