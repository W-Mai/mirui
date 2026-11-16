use super::input::circuit_tick_system;
use super::setup_app;
use super::state::CircuitSurface;
use crate::core::i18n::{I18n, Locale};
use crate::core::model::ModelHandle;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::circuit::{
    CircuitError, CircuitModal, CircuitModel, CircuitModelHandle, CircuitPage, GateKind,
    TASK_COUNT, TASKS,
};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{App, Entity, Fixed, Rect, RendererFactory, Surface};
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

fn dispatch(world: &mut World, event: &GestureEvent) -> bool {
    let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
    crate::__macro_support::flush_entity_events(world).unwrap();
    decision == crate::input::event::BubbleControl::Prevent
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
    for id in [
        "circuit_surface",
        "circuit_task",
        "circuit_add",
        "circuit_primary",
        "circuit_result_next",
    ] {
        assert!(world.find_by_id(id).is_some(), "{id}");
    }
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
            x: Fixed::from_int(188),
            y: Fixed::from_int(162),
            target: surface,
        },
        GestureEvent::DragMove {
            x: Fixed::from_int(223),
            y: Fixed::from_int(222),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface,
        },
        GestureEvent::DragCancel {
            x: Fixed::from_int(223),
            y: Fixed::from_int(222),
            target: surface,
        },
    ] {
        assert!(dispatch(&mut world, &event));
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
            x: Fixed::from_int(188),
            y: Fixed::from_int(162),
            target: surface,
        },
        GestureEvent::DragMove {
            x: Fixed::from_int(223),
            y: Fixed::from_int(222),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface,
        },
    ] {
        assert!(dispatch(&mut world, &event));
    }

    world.insert(surface, ComputedRect(Rect::new(0, 0, 0, 0)));
    let cancel = GestureEvent::DragCancel {
        x: Fixed::from_int(223),
        y: Fixed::from_int(222),
        target: surface,
    };
    assert!(!dispatch(&mut world, &cancel));
    assert_ne!(
        with_model(&world, |model| model.visual_gate_position(1).unwrap()),
        (before.x, before.y)
    );

    world.insert(surface, ComputedRect(rect));
    assert!(dispatch(&mut world, &cancel));
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

    assert_text(&world, "circuit_gates_line", "Gates 1 · par 1");
    assert_text(&world, "circuit_task", "☰ 01 · Invert");
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));

    model.set_page(CircuitPage::Trace);
    model.step_trace();
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));
    assert!(!world.has::<Hidden>(world.find_by_id("circuit_trace_page").unwrap()));
    assert_text(&world, "circuit_trace_count", "1 / 32");
    assert_text(&world, "circuit_trace_last", "0 → 0");

    model.load_task(10, false);
    flush(&mut world);
    assert_text(
        &world,
        "circuit_live_inputs",
        "0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1",
    );
    assert_text(&world, "circuit_live_expected", "0\n0\n0\n1\n0\n1\n1\n1");
    model.set_page(CircuitPage::Truth);
    flush(&mut world);
    assert_text(
        &world,
        "circuit_truth_result",
        "PASS\nPASS\nPASS\nFAIL\nPASS\nFAIL\nPASS\nPASS",
    );

    for id in [
        "circuit_gates_line",
        "circuit_task",
        "circuit_live_code",
        "circuit_live_inputs",
        "circuit_truth_result",
        "circuit_trace_count",
        "circuit_modal_title",
        "circuit_task_stars",
        "circuit_status",
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
    assert_text(&world, "circuit_status", TASKS[0].hint.en);

    model.verify();
    flush(&mut world);
    assert_text(&world, "circuit_status", "Y is not wired yet");

    model.load_task(1, false);
    model.verify();
    flush(&mut world);
    assert_text(
        &world,
        "circuit_status",
        "2/4 rows pass · at A=0 B=1, Y should be 0",
    );
    assert_text(&world, "circuit_primary", "☑ Check");

    model.set_selected_kind(GateKind::And).unwrap();
    model.verify();
    flush(&mut world);
    assert_eq!(model.modal(), CircuitModal::Result);
    assert_text(&world, "circuit_result_title", "Task 02 passed");
    assert_text(&world, "circuit_result_line", "1 gate · full marks");
    assert_text(&world, "circuit_result_next", "Next →");

    model.close_modal();
    flush(&mut world);
    assert_text(&world, "circuit_status", "★ All 4 rows pass");
    assert_text(&world, "circuit_primary", "Next →");
    assert_text(&world, "circuit_task", "☰ 02 · Both On ★★★");

    model.verify_or_next();
    flush(&mut world);
    assert_text(&world, "circuit_task", "☰ 03 · A Not B");
    assert_text(&world, "circuit_primary", "☑ Check");
}

#[test]
fn nand_tasks_show_the_rule_beside_the_board_and_on_the_add_button() {
    let mut world = fixture();
    let model = model_handle(&world);
    let badge = world.find_by_id("circuit_live_gates").unwrap();
    assert!(world.has::<Hidden>(badge));

    model.load_task(4, false);
    flush(&mut world);
    assert!(!world.has::<Hidden>(badge));
    assert_text(&world, "circuit_live_gates", "NAND only");
    assert_text(&world, "circuit_add", "＋ NAND");

    assert_eq!(model.add_gate(GateKind::And), Err(CircuitError::NotAllowed));
    model.start_add_gate();
    flush(&mut world);
    assert_text(&world, "circuit_gates_line", "Gates 2 · par 1");
}

#[test]
fn the_picker_adds_up_the_stars() {
    let mut world = fixture();
    let model = model_handle(&world);
    let grid = world.find_by_id("circuit_task_grid").unwrap();
    assert!(world.has::<Hidden>(grid));

    model.open_modal(CircuitModal::Tasks);
    flush(&mut world);
    assert!(!world.has::<Hidden>(grid));
    assert!(world.has::<Hidden>(world.find_by_id("circuit_wire_page").unwrap()));
    assert_text(&world, "circuit_modal_title", "Choose a task");
    assert_text(&world, "circuit_task_stars", "★ 0 / 36");

    model.choose_task(7);
    model.show_reference();
    model.verify();
    model.open_modal(CircuitModal::Tasks);
    flush(&mut world);
    assert_text(&world, "circuit_task_stars", "★ 3 / 36");
    assert_eq!(with_model(&world, CircuitModel::solved_mask), 1 << 7);
}

#[test]
fn labels_follow_the_page_language() {
    let mut world = fixture();
    let label = |world: &World, id: &'static str| {
        world
            .get::<Text>(world.find_by_id(id).unwrap())
            .unwrap()
            .resolve(world.resource())
            .into_owned()
    };
    assert_eq!(label(&world, "circuit_tab_wire"), "Wire");

    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    flush(&mut world);
    assert_eq!(label(&world, "circuit_tab_wire"), "布线");
    assert_text(&world, "circuit_task", "☰ 01 · 取反");
    assert_text(&world, "circuit_status", TASKS[0].hint.zh);
    assert_text(&world, "circuit_gates_line", "门数 1 · 最少 1");
    assert_text(&world, "circuit_primary", "☑ 验证");

    world.resource::<I18n>().unwrap().set_locale(Locale::EnUs);
    flush(&mut world);
    assert_text(&world, "circuit_task", "☰ 01 · Invert");
}

/// Taps whatever control sits at `(x, y)` on the rendered screen.
fn tap_at<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, root: Entity, x: i32, y: i32) {
    let (x, y) = (Fixed::from_int(x), Fixed::from_int(y));
    let target = crate::input::event::hit_test::hit_test(&app.world, root, x, y, 480, 320).unwrap();
    dispatch(&mut app.world, &GestureEvent::Tap { x, y, target });
    app.render_dirty().unwrap();
}

#[test]
fn a_pass_shows_its_stars_and_the_next_task_is_one_tap() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.render().unwrap();
    let model = model_handle(&app.world);
    model.show_reference();
    app.render_dirty().unwrap();
    tap_at(&mut app, root, 417, 276);
    assert_eq!(model.modal(), CircuitModal::Result);
    assert_eq!(model.stars()[0], 3);
    tap_at(&mut app, root, 276, 209);
    assert_eq!(model.task(), 1);
    assert_eq!(model.modal(), CircuitModal::None);
}

/// Every visible text fits its box; without a dialog over the page, no two
/// texts share pixels. The rules of the game_audit tour.
fn assert_screen_reads_cleanly(world: &World, modal_open: bool) {
    let fits = crate::gallery::demos::text_fits(world);
    for fit in &fits {
        assert!(!fit.overflows(4), "{:?} overflows {:?}", fit.text, fit.rect);
    }
    if modal_open {
        return;
    }
    let ink = |fit: &crate::gallery::demos::TextFit| {
        let mut lines = fit.lines;
        lines.y += Fixed::from_int(2);
        lines.h -= Fixed::from_int(5);
        lines
    };
    for (index, fit) in fits.iter().enumerate() {
        for other in &fits[index + 1..] {
            if fit.text.trim().is_empty() || other.text.trim().is_empty() {
                continue;
            }
            let (a, b) = (ink(fit), ink(other));
            assert!(
                !(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h),
                "{:?} overlaps {:?}",
                fit.text,
                other.text
            );
        }
    }
}

fn settle<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, modal_open: bool) {
    app.render_dirty().unwrap();
    assert_screen_reads_cleanly(&app.world, modal_open);
}

#[test]
fn every_page_task_and_dialog_reads_cleanly_in_both_languages() {
    for locale in [Locale::EnUs, Locale::ZhCn] {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        app.world.resource::<I18n>().unwrap().set_locale(locale);
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.render().unwrap();
        let model = model_handle(&app.world);
        for task in 0..TASK_COUNT as u8 {
            model.load_task(task, true);
            for page in [CircuitPage::Wire, CircuitPage::Truth, CircuitPage::Trace] {
                model.set_page(page);
                model.step_trace();
                settle(&mut app, false);
            }
            model.set_page(CircuitPage::Wire);
            model.verify();
            settle(&mut app, true);
            model.close_modal();
            settle(&mut app, false);
            model.load_task(task, false);
            model.verify();
            settle(&mut app, false);
            model.start_edit_gate();
            settle(&mut app, model.modal() != CircuitModal::None);
            model.close_modal();
        }
        model.load_task(0, false);
        model.select_source(crate::gallery::play::circuit::SignalSource::gate(1));
        model.connect_from_tap(Some(1), 0);
        settle(&mut app, false);
        for modal in [
            CircuitModal::Tasks,
            CircuitModal::Help,
            CircuitModal::GateTypes { adding: true },
            CircuitModal::GateTypes { adding: false },
        ] {
            model.open_modal(modal);
            settle(&mut app, true);
            model.close_modal();
        }
    }
}
