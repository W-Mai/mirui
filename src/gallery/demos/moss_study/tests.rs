use super::input::{local_cell, moss_tick_system};
use super::state::MossSurface;
use super::{VIEWPORT, setup_app};
use crate::ecs::{DeltaTimeMs, SystemScheduler};
use crate::gallery::play::moss::MossModel;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{ComputedRect, Hidden};

fn fixture() -> World {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&MossModel) -> R) -> R {
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = &world.get::<MossSurface>(surface).unwrap().model;
    crate::core::model::ModelHandle::read(model, inspect)
}

fn trigger(world: &mut World, surface: Entity, event: &GestureEvent) -> bool {
    GestureHandler::trigger(world, surface, event).unwrap_or(false)
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert_eq!(text.resolve(world), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("moss_surface").is_some());
    assert!(world.find_by_id("moss_run").is_some());
    assert!(world.find_by_id("moss_seed_2").is_some());
    assert_eq!(world.query::<MossSurface>().collect().len(), 1);
}

#[test]
fn cancelled_drag_restores_the_whole_garden_transaction() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    assert_eq!(
        local_cell(rect, Fixed::from_int(16), Fixed::from_int(67)),
        None
    );
    let before = with_model(&world, |model| *model.cells());
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(18),
            y: Fixed::from_int(68),
            target: surface,
        },
    ));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragMove {
            x: Fixed::from_int(350),
            y: Fixed::from_int(265),
            dx: Fixed::from_int(332),
            dy: Fixed::from_int(197),
            target: surface,
        },
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(350),
            y: Fixed::from_int(265),
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, |model| *model.cells()), before);
}

#[test]
fn visual_changes_dirty_the_bound_surface_without_node_sync() {
    use crate::gallery::play::moss::MossTool;
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let modal = world.find_by_id("moss_modal").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);
    world.remove::<VisualDirty>(modal);

    world
        .get::<MossSurface>(surface)
        .unwrap()
        .model
        .set_tool(MossTool::Erase);
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert!(world.has::<VisualDirty>(surface));
    assert!(!world.has::<VisualDirty>(modal));
}

#[test]
fn observed_text_and_modal_branches_follow_the_model() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let modal = world.find_by_id("moss_modal").unwrap();
    let seed = world.find_by_id("moss_seed_0").unwrap();
    let clear = world.find_by_id("moss_clear_note").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();

    assert_text(&world, "moss_rate", "4 代/秒 ↻");
    assert!(world.has::<Hidden>(modal));
    model.cycle_rate();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_text(&world, "moss_rate", "8 代/秒 ↻");

    model.open_seeds();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!world.has::<Hidden>(modal));
    assert!(!world.has::<Hidden>(seed));
    assert!(world.has::<Hidden>(clear));
    assert_text(&world, "moss_modal_title", "给花园一种新的开始");
    assert_text(
        &world,
        "moss_modal_subtitle",
        "载入会暂停演化；新种子可以撤销。",
    );

    model.open_clear();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(world.has::<Hidden>(seed));
    assert!(!world.has::<Hidden>(clear));
    assert_text(&world, "moss_modal_title", "让花园重新开始？");
    assert_text(
        &world,
        "moss_modal_subtitle",
        "当前有 15 个活细胞；清空后代数归零。",
    );
}

#[test]
fn gestures_commit_once_and_bound_tick_updates_readouts() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();

    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::Tap {
            x: Fixed::from_int(18),
            y: Fixed::from_int(68),
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, MossModel::history_len), 1);

    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(35),
            y: Fixed::from_int(68),
            target: surface,
        },
    ));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragMove {
            x: Fixed::from_int(69),
            y: Fixed::from_int(68),
            dx: Fixed::from_int(34),
            dy: Fixed::ZERO,
            target: surface,
        },
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(69),
            y: Fixed::from_int(68),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        },
    ));
    assert_eq!(with_model(&world, MossModel::history_len), 2);

    model.toggle_running();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(moss_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world);
    assert_eq!(with_model(&world, MossModel::generation), 0);

    world.insert_resource(DeltaTimeMs(250));
    scheduler.run_all(&mut world);
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert_eq!(with_model(&world, MossModel::generation), 1);
    assert_text(&world, "moss_generation", "001");
    let live = with_model(&world, MossModel::live_count);
    let live_text = world
        .get::<Text>(world.find_by_id("moss_live").unwrap())
        .unwrap()
        .resolve(&world);
    assert_eq!(live_text.as_ref().parse::<u16>(), Ok(live));
}

#[test]
#[should_panic(expected = "missing system resource `DeltaTimeMs`")]
fn bound_tick_requires_the_frame_delta_resource() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(moss_tick_system::system(model));

    scheduler.run_all(&mut world);
}
