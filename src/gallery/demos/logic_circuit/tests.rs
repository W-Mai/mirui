use super::input::circuit_tick_system;
use super::setup_app;
use super::state::CircuitSurface;
use crate::core::model::ModelHandle;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::circuit::{
    CircuitError, CircuitModal, CircuitModel, CircuitModelHandle, CircuitPage, GateKind, TASKS,
};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{App, Fixed, Rect};
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
    assert_eq!(text.resolve(world.resource()).as_ref(), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

#[test]
fn bound_tick_uses_delta_time_and_zero_elapsed_is_idle() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.set_page(CircuitPage::Trace);
    model.toggle_scanning();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(circuit_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(120));
    for _ in 0..5 {
        scheduler.run_all(&mut world).unwrap();
    }
    assert_eq!(model.trace_len(), 1);

    let revision = model.visual_revision();
    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(model.trace_len(), 1);
    assert_eq!(model.visual_revision(), revision);
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
            x: Fixed::from_int(170),
            y: Fixed::from_int(127),
            target: surface,
        },
        GestureEvent::DragMove {
            x: Fixed::from_int(205),
            y: Fixed::from_int(187),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface,
        },
        GestureEvent::DragCancel {
            x: Fixed::from_int(205),
            y: Fixed::from_int(187),
            target: surface,
        },
    ] {
        assert!({
            let decision =
                crate::__macro_support::dispatch_entity_gesture(&mut world, &event, 0).unwrap();
            crate::__macro_support::flush_entity_events(&mut world).unwrap();
            decision == crate::input::event::BubbleControl::Prevent
        });
    }
    assert_eq!(
        with_model(&world, |model| model.gate_by_id(1).unwrap()),
        before
    );
}

#[test]
fn gate_drag_end_waits_for_a_valid_surface_rect() {
    let mut world = fixture();
    let surface = world.find_by_id("circuit_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    let before = with_model(&world, |model| model.gate_by_id(1).unwrap());

    for event in [
        GestureEvent::DragStart {
            x: Fixed::from_int(170),
            y: Fixed::from_int(127),
            target: surface,
        },
        GestureEvent::DragMove {
            x: Fixed::from_int(205),
            y: Fixed::from_int(187),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface,
        },
    ] {
        assert!({
            let decision =
                crate::__macro_support::dispatch_entity_gesture(&mut world, &event, 0).unwrap();
            crate::__macro_support::flush_entity_events(&mut world).unwrap();
            decision == crate::input::event::BubbleControl::Prevent
        });
    }

    world.insert(surface, ComputedRect(Rect::new(0, 0, 0, 0)));
    let cancel = GestureEvent::DragCancel {
        x: Fixed::from_int(205),
        y: Fixed::from_int(187),
        target: surface,
    };
    assert!(!{
        let decision =
            crate::__macro_support::dispatch_entity_gesture(&mut world, &cancel, 0).unwrap();
        crate::__macro_support::flush_entity_events(&mut world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    });
    assert_ne!(
        with_model(&world, |model| model.visual_gate_position(1).unwrap()),
        (before.x, before.y)
    );

    world.insert(surface, ComputedRect(rect));
    assert!({
        let decision =
            crate::__macro_support::dispatch_entity_gesture(&mut world, &cancel, 0).unwrap();
        crate::__macro_support::flush_entity_events(&mut world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    });
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
    model.load_task(10, true);
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

    assert_text(&world, "circuit_gate_count", "NET · 1 / 6 GATES");
    assert_text(&world, "circuit_task", "任务 01 · 取反");
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));

    model.set_page(CircuitPage::Trace);
    model.step_trace();
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_trace_page").unwrap()));
    assert_text(&world, "circuit_trace_count", "1 / 32");
    assert_text(&world, "circuit_trace_last", "0 → 0");

    model.load_task(10, false);
    model.set_page(CircuitPage::Truth);
    flush(&mut world);
    assert_text(&world, "circuit_truth_a", "0\n0\n0\n0\n1\n1\n1\n1");
    assert_text(&world, "circuit_truth_c", "0\n1\n0\n1\n0\n1\n0\n1");
    assert_text(&world, "circuit_truth_expected", "0\n0\n0\n1\n0\n1\n1\n1");
    assert_text(
        &world,
        "circuit_truth_result",
        "PASS\nPASS\nPASS\nFAIL\nPASS\nFAIL\nPASS\nPASS",
    );

    for id in [
        "circuit_gate_count",
        "circuit_task",
        "circuit_input_0",
        "circuit_gate_0",
        "circuit_output",
        "circuit_live_0",
        "circuit_truth_result",
        "circuit_trace_count",
        "circuit_modal_title",
        "circuit_task_solved",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn status_line_reads_the_hint_then_the_verdict() {
    let mut world = fixture();
    let model = model_handle(&world);
    assert_text(&world, "circuit_wire_status", TASKS[0].hint);

    model.verify();
    flush(&mut world);
    assert_text(&world, "circuit_wire_status", "Y 还没接线");

    model.load_task(1, false);
    model.verify();
    flush(&mut world);
    assert_text(
        &world,
        "circuit_wire_status",
        "2/4 行通过 · A=0 B=1 时 Y 应为 0",
    );
    assert_text(&world, "circuit_footer_4", "☑ 验证");

    model.set_selected_kind(GateKind::And).unwrap();
    model.verify();
    flush(&mut world);
    assert_text(&world, "circuit_wire_status", "★ 全部 4 行通过");
    assert_text(&world, "circuit_footer_4", "下一题 →");
    assert_text(&world, "circuit_task", "任务 02 · 都亮才亮 ★");

    model.verify_or_next();
    flush(&mut world);
    assert_text(&world, "circuit_task", "任务 03 · A 开 B 关");
    assert_text(&world, "circuit_footer_4", "☑ 验证");
}

#[test]
fn nand_tasks_show_the_rule_on_the_board_and_the_buttons() {
    let mut world = fixture();
    let model = model_handle(&world);
    let badge = world.find_by_id("circuit_live_gates").unwrap();
    assert!(world.has::<Hidden>(badge));

    model.load_task(4, false);
    flush(&mut world);
    assert!(!world.has::<Hidden>(badge));
    assert_text(&world, "circuit_live_gates", "只用 NAND");
    assert_text(&world, "circuit_footer_0", "＋ NAND");
    assert_text(&world, "circuit_footer_1", "删除");

    assert_eq!(model.add_gate(GateKind::And), Err(CircuitError::NotAllowed));
    model.start_add_gate();
    flush(&mut world);
    assert_text(&world, "circuit_gate_count", "NET · 2 / 6 GATES");
}

#[test]
fn the_picker_counts_solved_tasks() {
    let mut world = fixture();
    let model = model_handle(&world);
    let grid = world.find_by_id("circuit_task_grid").unwrap();
    assert!(world.has::<Hidden>(grid));

    model.open_modal(CircuitModal::Tasks);
    flush(&mut world);
    assert!(!world.has::<Hidden>(grid));
    assert!(world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));
    assert_text(&world, "circuit_modal_title", "选择任务");
    assert_text(&world, "circuit_task_solved", "★ 0 / 12");

    model.choose_task(7);
    model.show_reference();
    model.verify();
    model.open_modal(CircuitModal::Tasks);
    flush(&mut world);
    assert_text(&world, "circuit_task_solved", "★ 1 / 12");
    assert_eq!(with_model(&world, CircuitModel::solved_mask), 1 << 7);
}
