use super::input::local_cell;
use super::state::TideSurface;
use super::{VIEWPORT, setup_app};
use crate::core::model::ModelHandle;
use crate::ecs::World;
use crate::gallery::play::tidal::{BOARD_SIZE, TideModal, TideModel, TideModelHandle};
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

fn model_handle(world: &World) -> TideModelHandle {
    let surface = world.find_by_id("tide_surface").unwrap();
    world.get::<TideSurface>(surface).unwrap().model.clone()
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&TideModel) -> R) -> R {
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

fn is_hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("tide_surface").is_some());
    assert!(world.find_by_id("tide_offer_0").is_some());
    assert!(world.find_by_id("tide_place").is_some());
    assert_eq!(world.query::<TideSurface>().collect().len(), 1);
}

#[test]
fn board_hit_testing_and_gesture_use_the_bound_model() {
    let mut world = fixture();
    let surface = world.find_by_id("tide_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    assert_eq!(
        local_cell(rect, Fixed::from_int(14), Fixed::from_int(60)),
        Some(0)
    );
    assert_eq!(
        local_cell(rect, Fixed::from_int(46), Fixed::from_int(60)),
        None
    );

    let valid = with_model(&world, |model| {
        (0..BOARD_SIZE).find(|index| model.valid(*index)).unwrap()
    });
    let x = 14 + (valid % 6) as i32 * 35 + 16;
    let y = 60 + (valid / 6) as i32 * 35 + 16;
    assert_eq!(with_model(&world, TideModel::pending), None);
    assert!({
        let decision = crate::__macro_support::dispatch_entity_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
                target: surface,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    });
    assert_eq!(with_model(&world, TideModel::pending), Some(valid as u8));
}

#[test]
fn board_and_modal_observations_dirty_only_their_bound_views() {
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture();
    let surface = world.find_by_id("tide_surface").unwrap();
    let modal = world.find_by_id("tide_modal").unwrap();
    let model = model_handle(&world);
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);
    world.remove::<VisualDirty>(modal);

    model.set_modal(TideModal::Voyage);
    flush(&mut world);
    assert!(!world.has::<VisualDirty>(surface));
    assert!(world.has::<VisualDirty>(modal));

    world.remove::<VisualDirty>(surface);
    world.remove::<VisualDirty>(modal);
    let valid = with_model(&world, |model| {
        (0..BOARD_SIZE).find(|index| model.valid(*index)).unwrap() as u8
    });
    model.select_cell(valid);
    flush(&mut world);
    assert!(world.has::<VisualDirty>(surface));
}

#[test]
fn observed_state_updates_bounded_text_and_visibility() {
    let mut world = fixture();
    let model = model_handle(&world);

    assert_text(&world, "tide_chapter", "第 1 / 4 岛");
    assert_text(&world, "tide_turn", "落子 0 / 24");
    assert!(is_hidden(&world, "tide_modal"));
    assert!(is_hidden(&world, "tide_place"));

    let valid = with_model(&world, |model| {
        (0..BOARD_SIZE).find(|index| model.valid(*index)).unwrap() as u8
    });
    model.select_cell(valid);
    flush(&mut world);
    assert!(!is_hidden(&world, "tide_place"));
    assert!(is_hidden(&world, "tide_reroll"));
    let preview = world
        .get::<Text>(world.find_by_id("tide_preview").unwrap())
        .unwrap();
    assert!(!preview.resolve(world.resource()).is_empty());
    assert!(preview.has_valid_content());

    model.cancel_preview();
    model.set_modal(TideModal::Voyage);
    flush(&mut world);
    assert!(!is_hidden(&world, "tide_modal"));
    assert!(!is_hidden(&world, "tide_voyage_0"));
    assert!(is_hidden(&world, "tide_modal_score"));
    assert_text(&world, "tide_modal_title", "四岛航行图");
}

#[cfg(feature = "persistence")]
#[test]
fn persistent_actions_append_inside_the_model_update() {
    let world = fixture();
    let model = model_handle(&world);
    let index = with_model(&world, |model| {
        (0..BOARD_SIZE).find(|index| model.valid(*index)).unwrap() as u8
    });

    model.place(index, 0);
    ModelHandle::read(&model, |model| {
        assert_eq!(model.replay_len(), 1);
        assert_eq!(model.turn(), 1);
    });
    model.place(u8::MAX, 0);
    ModelHandle::read(&model, |model| {
        assert_eq!(model.replay_len(), 1);
        assert_eq!(model.turn(), 1);
    });
}

fn settle_island(model: &TideModelHandle) {
    use crate::gallery::play::tidal::tests::best_move;
    while !ModelHandle::read(model, TideModel::settled) {
        let (index, choice) = ModelHandle::read(model, best_move);
        model.place(index, choice);
    }
}

fn assert_texts_fit(world: &World) {
    for fit in crate::gallery::demos::text_fits(world) {
        assert!(!fit.overflows(4), "{:?} in {:?}", fit.text, fit.rect);
    }
}

#[test]
fn a_whole_voyage_reads_and_fits_from_first_tile_to_the_totals() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    let model = model_handle(&app.world);
    assert_text(&app.world, "tide_goal", "邻里计划 · 聚落");
    assert_text(&app.world, "tide_until", "再落 6 块结算 · 目标 430");

    for island in 0..4 {
        settle_island(&model);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        assert!(!is_hidden(&app.world, "tide_result"));
        assert!(is_hidden(&app.world, "tide_cancel"));
        assert!(is_hidden(&app.world, "tide_reroll"));
        assert_text(
            &app.world,
            "tide_result",
            if island == 3 {
                "本岛结算 · 完成航行"
            } else {
                "本岛结算 · 选择新学说"
            },
        );
        assert_text(
            &app.world,
            "tide_until",
            &alloc::format!("本岛已结算 · 目标 {}", 430 + island * 55),
        );
        assert_text(&app.world, "tide_estimate", "");

        model.set_modal(TideModal::Result);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        assert_text(
            &app.world,
            "tide_modal_subtitle",
            if island == 3 {
                "最后一座岛已结算。"
            } else {
                "选一项学说，带去下一座岛。"
            },
        );
        let perk = (island < 3).then(|| model.perk_offers()[0]);
        model.continue_voyage(perk);
    }

    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    let (total, scores) = with_model(&app.world, |model| {
        (model.total(), model.results().map(|result| result.score))
    });
    assert_text(&app.world, "tide_modal_title", "远航完成");
    assert_text(&app.world, "tide_modal_subtitle", "四座岛的总分。");
    assert_text(&app.world, "tide_modal_score", &alloc::format!("{total}"));
    assert_text(&app.world, "tide_total", &alloc::format!("累计 {total}"));
    assert_text(
        &app.world,
        "tide_modal_detail",
        &alloc::format!(
            "四岛 {} / {} / {} / {}",
            scores[0],
            scores[1],
            scores[2],
            scores[3]
        ),
    );
    assert_text(&app.world, "tide_message", "四岛航行完成。");
}
