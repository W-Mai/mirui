use super::input::{local_cell, moss_tick_system};
use super::state::MossSurface;
use super::{VIEWPORT, setup_app};
use crate::core::i18n::{I18n, Locale};
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::moss::{MossModal, MossModel, MossTool};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::{dispatch_input, input::PhysicalKey};
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

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    }
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert_eq!(text.resolve(world.resource()), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
}

fn press(world: &mut World, key: PhysicalKey, at: u32) {
    dispatch_input(
        world,
        None,
        &crate::test_support::physical_key(key).into(),
        at,
        1,
        1,
    );
}

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("moss_surface").is_some());
    for id in [
        "moss_tool_plant",
        "moss_tool_erase",
        "moss_tool_glider",
        "moss_run",
        "moss_step",
        "moss_rate",
        "moss_undo",
        "moss_menu",
        "moss_seed_2",
    ] {
        assert!(world.find_by_id(id).is_some(), "{id}");
    }
    assert_eq!(world.query::<MossSurface>().collect().len(), 1);
}

#[test]
fn the_garden_maps_taps_onto_its_cells_and_nothing_around_them() {
    let rect = Rect::new(0, 0, 480, 320);
    let cell = |x: i32, y: i32| local_cell(rect, Fixed::from_int(x), Fixed::from_int(y));
    assert_eq!(cell(10, 38), Some((0, 0)));
    assert_eq!(cell(469, 313), Some((19, 11)));
    assert_eq!(cell(9, 100), None);
    assert_eq!(cell(470, 100), None);
    assert_eq!(cell(100, 37), None);
    assert_eq!(cell(100, 314), None);
    let doubled = Rect::new(20, 10, 960, 640);
    assert_eq!(
        local_cell(
            doubled,
            Fixed::from_int(20 + 2 * 33),
            Fixed::from_int(10 + 2 * 61)
        ),
        Some((1, 1))
    );
}

#[test]
fn cancelled_drag_restores_the_whole_garden_transaction() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    let before = with_model(&world, |model| *model.cells());
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(11),
            y: Fixed::from_int(39),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragMove {
            x: Fixed::from_int(469),
            y: Fixed::from_int(313),
            dx: Fixed::from_int(458),
            dy: Fixed::from_int(274),
            target: surface,
        }
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(469),
            y: Fixed::from_int(313),
            target: surface,
        }
    ));
    assert_eq!(with_model(&world, |model| *model.cells()), before);
}

#[test]
fn visual_changes_dirty_the_bound_surface_without_node_sync() {
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
    let clear = world.find_by_id("moss_clear_confirm").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();

    assert_text(&world, "moss_rate", "4/s");
    assert!(world.has::<Hidden>(modal));
    model.cycle_rate();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_text(&world, "moss_rate", "8/s");

    model.open_seeds();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!world.has::<Hidden>(modal));
    assert!(!crate::ui::branch::is_hidden_in_tree(&world, seed));
    assert!(crate::ui::branch::is_hidden_in_tree(&world, clear));
    assert_text(&world, "moss_modal_title", "Seeds");
    assert_text(
        &world,
        "moss_modal_subtitle",
        "Loads paused. Undo takes it back.",
    );

    model.open_clear();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(crate::ui::branch::is_hidden_in_tree(&world, seed));
    assert!(!crate::ui::branch::is_hidden_in_tree(&world, clear));
    assert_text(&world, "moss_modal_title", "Clear the garden?");
    assert_text(
        &world,
        "moss_modal_subtitle",
        "15 live cells go; generation back to 0.",
    );
}

#[test]
fn seed_cards_load_their_seed() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();
    model.open_seeds();
    crate::core::reactive::flush_signal_dirty(&mut world);
    let card = world.find_by_id("moss_seed_card_1").unwrap();
    assert!(!crate::ui::branch::is_hidden_in_tree(&world, card));
    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: card,
        }
    ));
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(with_model(&world, MossModel::seed_id), 1);
    assert_eq!(with_model(&world, MossModel::modal), MossModal::None);
    assert_text(&world, "moss_tool_glider", "↘");
    assert_text(&world, "moss_run", "▶");
}

#[test]
fn the_glider_button_shows_where_the_next_glider_flies() {
    let mut world = fixture();
    let glider = world.find_by_id("moss_tool_glider").unwrap();
    for arrow in ["↘", "↙", "↖", "↗", "↘"] {
        assert!(trigger(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: glider,
            }
        ));
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert_text(&world, "moss_tool_glider", arrow);
    }
    assert_eq!(with_model(&world, MossModel::tool), MossTool::Glider);
}

#[test]
fn gestures_commit_once_and_bound_tick_updates_readouts() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();

    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(11),
            y: Fixed::from_int(39),
            target: surface,
        }
    ));
    assert_eq!(with_model(&world, MossModel::history_len), 1);

    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(35),
            y: Fixed::from_int(39),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragMove {
            x: Fixed::from_int(81),
            y: Fixed::from_int(39),
            dx: Fixed::from_int(46),
            dy: Fixed::ZERO,
            target: surface,
        }
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(81),
            y: Fixed::from_int(39),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_eq!(with_model(&world, MossModel::history_len), 2);

    model.toggle_running();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(moss_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(with_model(&world, MossModel::generation), 0);

    world.insert_resource(DeltaTimeMs(250));
    scheduler.run_all(&mut world).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert_eq!(with_model(&world, MossModel::generation), 1);
    assert_text(&world, "moss_generation", "Gen 001");
    let live = with_model(&world, MossModel::live_count);
    assert_text(&world, "moss_live", &alloc::format!("{live} live"));
}

#[test]
#[should_panic(expected = "requires resource")]
fn bound_tick_requires_the_frame_delta_resource() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(moss_tick_system::system(model));

    scheduler.run_all(&mut world).unwrap();
}

#[test]
fn every_word_follows_the_page_language() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();
    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_text(&world, "moss_title", "苔藓研究");
    assert_text(&world, "moss_generation", "第 000 代");
    assert_text(&world, "moss_live", "存活 15");
    assert_text(&world, "moss_rate", "4代/秒");

    model.open_clear();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_text(&world, "moss_modal_title", "清空花园？");
    assert_text(
        &world,
        "moss_modal_subtitle",
        "15 个活细胞会被清除，代数归零。",
    );
    assert_text(&world, "moss_clear_badge", "可撤销");

    world.resource::<I18n>().unwrap().set_locale(Locale::EnUs);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_text(&world, "moss_modal_title", "Clear the garden?");
    assert_text(&world, "moss_rate", "4/s");
}

#[test]
fn keys_run_step_undo_and_pick_tools() {
    let mut world = fixture();
    let surface = world.find_by_id("moss_surface").unwrap();
    let model = world.get::<MossSurface>(surface).unwrap().model.clone();
    press(&mut world, PhysicalKey::KeyN, 0);
    assert_eq!(model.generation(), 1);
    press(&mut world, PhysicalKey::KeyZ, 1);
    assert_eq!(model.generation(), 0);
    press(&mut world, PhysicalKey::Space, 2);
    assert!(model.running());
    press(&mut world, PhysicalKey::Digit2, 3);
    assert_eq!(model.tool(), MossTool::Erase);
    press(&mut world, PhysicalKey::KeyR, 4);
    press(&mut world, PhysicalKey::KeyR, 5);
    assert_eq!((model.tool(), model.rotation()), (MossTool::Glider, 1));
    press(&mut world, PhysicalKey::Escape, 6);
    assert_eq!(model.modal(), MossModal::Seeds);
    press(&mut world, PhysicalKey::KeyN, 7);
    assert_eq!(model.generation(), 0, "keys wait while the seeds are open");
    press(&mut world, PhysicalKey::Escape, 8);
    assert_eq!(model.modal(), MossModal::None);
}
