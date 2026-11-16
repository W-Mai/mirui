use super::input::local_cell;
use super::state::TideSurface;
use super::style::{BOARD_X, BOARD_Y, CELL, PITCH};
use super::{VIEWPORT, setup_app};
use crate::core::i18n::{I18n, Locale};
use crate::core::model::ModelHandle;
use crate::ecs::World;
use crate::gallery::play::tidal::{BOARD_SIZE, TideModal, TideModel, TideModelHandle};
use crate::input::event::dispatch_input;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{ComputedRect, Hidden};

type HeadlessApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

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

fn text_of(world: &World, id: &'static str) -> String {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("text widget");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
    text.resolve(world.resource()).to_string()
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    assert_eq!(text_of(world, id), expected, "{id}");
}

fn is_hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

fn first_valid(world: &World) -> u8 {
    with_model(world, |model| {
        (0..BOARD_SIZE).find(|index| model.valid(*index)).unwrap() as u8
    })
}

/// Taps a widget the way a finger would reach its handler.
fn tap(world: &mut World, target: Entity, x: i32, y: i32) {
    crate::__macro_support::dispatch_entity_gesture(
        world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target,
        },
        0,
    )
    .unwrap();
    crate::__macro_support::flush_entity_events(world).unwrap();
}

fn tap_id(world: &mut World, id: &'static str) {
    let target = world.find_by_id(id).unwrap();
    tap(world, target, 0, 0);
}

fn tap_cell(world: &mut World, cell: u8) {
    let surface = world.find_by_id("tide_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let x = BOARD_X + i32::from(cell % 6) * PITCH + CELL / 2;
    let y = BOARD_Y + i32::from(cell / 6) * PITCH + CELL / 2;
    tap(world, surface, x, y);
}

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("tide_surface").is_some());
    assert!(world.find_by_id("tide_offers").is_some());
    assert!(world.find_by_id("tide_place").is_some());
    assert!(world.find_by_id("tide_gains").is_some());
    assert_eq!(world.query::<TideSurface>().collect().len(), 1);
}

#[test]
fn board_hit_testing_previews_on_the_first_tap_and_places_on_the_second() {
    let mut world = fixture();
    let rect = Rect::new(0, 0, 480, 320);
    let at = |x: i32, y: i32| local_cell(rect, Fixed::from_int(x), Fixed::from_int(y));
    assert_eq!(at(BOARD_X, BOARD_Y), Some(0));
    assert_eq!(at(BOARD_X + CELL + 1, BOARD_Y), None);
    assert_eq!(
        at(BOARD_X + 5 * PITCH + CELL - 1, BOARD_Y + 5 * PITCH),
        Some(35)
    );
    assert_eq!(at(BOARD_X + 6 * PITCH, BOARD_Y), None);

    let valid = first_valid(&world);
    assert_eq!(with_model(&world, TideModel::pending), None);
    tap_cell(&mut world, valid);
    assert_eq!(with_model(&world, TideModel::pending), Some(valid));
    assert_eq!(with_model(&world, TideModel::turn), 0);
    tap_cell(&mut world, valid);
    assert_eq!(with_model(&world, TideModel::turn), 1);
    assert_eq!(
        with_model(&world, |model| model.dropping().map(|(cell, _)| cell)),
        Some(usize::from(valid))
    );
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
    model.select_cell(first_valid(&world));
    flush(&mut world);
    assert!(world.has::<VisualDirty>(surface));
}

#[test]
fn observed_state_updates_bounded_text_and_visibility() {
    let mut world = fixture();
    let model = model_handle(&world);

    assert_text(&world, "tide_chapter", "ISLAND 1/4 · TILE 0/24");
    assert_text(&world, "tide_until", "IN 6");
    assert_text(&world, "tide_target", "TARGET 430");
    assert_text(&world, "tide_place", "Place");
    assert_text(&world, "tide_preview", "");
    assert!(is_hidden(&world, "tide_modal"));
    assert!(!is_hidden(&world, "tide_place"));
    assert!(is_hidden(&world, "tide_result"));

    model.place_pending();
    flush(&mut world);
    assert_text(&world, "tide_message", "Tap sea next to land first.");

    model.select_cell(first_valid(&world));
    flush(&mut world);
    let place = text_of(&world, "tide_place");
    assert!(
        place.starts_with("Place +") || place.starts_with("Place -"),
        "{place}"
    );
    assert!(!text_of(&world, "tide_preview").is_empty());

    model.cancel_preview();
    model.set_modal(TideModal::Voyage);
    flush(&mut world);
    assert!(!is_hidden(&world, "tide_modal"));
    assert!(!is_hidden(&world, "tide_chart"));
    assert!(is_hidden(&world, "tide_modal_score"));
    assert_text(&world, "tide_modal_title", "Voyage chart");
    assert_text(&world, "tide_chart_best", "BEST —");
}

#[test]
fn every_label_follows_the_page_language() {
    let mut world = fixture();
    assert_text(&world, "tide_reroll", "Redraw · 3");
    assert_text(&world, "tide_goal", "Village plan · Hamlet");
    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    flush(&mut world);
    assert_text(&world, "tide_chapter", "第 1/4 岛 · 落子 0/24");
    assert_text(&world, "tide_reroll", "换牌 · 3");
    assert_text(&world, "tide_goal", "邻里计划 · 聚落");
    assert_text(&world, "tide_info_title", "待落位 · 灯塔");
    assert_text(&world, "tide_message", "点岛边的海放下地块。");
    assert_text(&world, "tide_forecast", "退潮 · 晴朗");
}

#[test]
fn a_harvest_pops_each_tiles_score_and_fades() {
    use crate::gallery::play::tidal::tests::best_move;

    let mut world = fixture();
    let model = model_handle(&world);
    for _ in 0..6 {
        let (index, choice) = ModelHandle::read(&model, best_move);
        model.place(index, choice);
    }
    for _ in 0..6 {
        model.advance_ms(16);
    }
    flush(&mut world);
    let gains = model.gains();
    let scored = (0..BOARD_SIZE).find(|index| gains[*index] > 0).unwrap();
    assert!(model.gain_lift() > 0);
    let pops: Vec<String> = {
        let mut texts = Vec::new();
        let mut queue = vec![world.find_by_id("tide_gains").unwrap()];
        while let Some(entity) = queue.pop() {
            if let Some(text) = world.get::<Text>(entity)
                && !crate::ui::branch::is_hidden_in_tree(&world, entity)
            {
                texts.push(text.resolve(world.resource()).to_string());
            }
            if let Some(children) = world.get::<crate::ui::Children>(entity) {
                queue.extend(children.0.iter().copied());
            }
        }
        texts
    };
    assert!(
        pops.contains(&alloc::format!("+{}", gains[scored])),
        "{pops:?}"
    );
    assert_eq!(
        gains.iter().map(|gain| u16::from(*gain)).sum::<u16>(),
        model.harvests()[0]
    );
    for _ in 0..200 {
        model.advance_ms(16);
    }
    flush(&mut world);
    assert!(!model.animating());
    assert_eq!(model.gain_lift(), 0);
    assert!(model.gains().iter().all(|gain| *gain == 0));
}

#[test]
fn keys_pick_walk_place_and_undo() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    let model = model_handle(&app.world);
    let mut press = |key: PhysicalKey| {
        dispatch_input(
            &mut app.world,
            None,
            &crate::test_support::physical_key(key).into(),
            0,
            480,
            320,
        );
    };
    press(PhysicalKey::Digit3);
    assert_eq!(model.choice(), 2);
    press(PhysicalKey::ArrowRight);
    let first = model.pending().expect("arrows pick a legal spot");
    press(PhysicalKey::ArrowDown);
    let next = model.pending().unwrap();
    assert!(next / 6 > first / 6, "{first} -> {next}");
    press(PhysicalKey::Enter);
    assert_eq!(model.turn(), 1);
    press(PhysicalKey::KeyZ);
    assert_eq!(model.turn(), 0);
    press(PhysicalKey::KeyR);
    assert_eq!(model.rerolls(), 2);
    press(PhysicalKey::KeyV);
    assert_eq!(model.modal(), TideModal::Voyage);
    press(PhysicalKey::Escape);
    assert_eq!(model.modal(), TideModal::None);
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

/// Plays a whole voyage through its buttons in `locale`, checking that every
/// text fits, and returns the world with the totals on show.
fn sail(locale: Locale) -> (HeadlessApp, TideModelHandle) {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.world.resource::<I18n>().unwrap().set_locale(locale);
    app.render().unwrap();
    let model = model_handle(&app.world);
    for island in 0..4 {
        settle_island(&model);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        assert!(!is_hidden(&app.world, "tide_result"));
        assert!(is_hidden(&app.world, "tide_place"));
        assert!(is_hidden(&app.world, "tide_reroll"));
        assert_text(&app.world, "tide_estimate", "");
        tap_id(&mut app.world, "tide_result");
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        if island < 3 {
            assert!(!is_hidden(&app.world, "tide_perks"));
            let card = app
                .world
                .get::<crate::ui::Children>(app.world.find_by_id("tide_perks").unwrap())
                .unwrap()
                .0[0];
            tap(&mut app.world, card, 0, 0);
        } else {
            tap_id(&mut app.world, "tide_finish");
        }
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
    }
    (app, model)
}

#[test]
fn a_whole_voyage_reads_and_fits_from_first_tile_to_the_totals() {
    let (mut app, model) = sail(Locale::EnUs);
    let (total, scores) = with_model(&app.world, |model| {
        (model.total(), model.results().map(|result| result.score))
    });
    assert!(model.complete());
    assert_text(&app.world, "tide_modal_title", "Voyage complete");
    assert_text(
        &app.world,
        "tide_modal_subtitle",
        "Total of all four islands.",
    );
    assert_text(&app.world, "tide_modal_score", &alloc::format!("{total}"));
    assert_text(&app.world, "tide_total", &alloc::format!("TOTAL {total}"));
    assert_text(
        &app.world,
        "tide_modal_detail",
        &alloc::format!(
            "Islands {} / {} / {} / {}",
            scores[0],
            scores[1],
            scores[2],
            scores[3]
        ),
    );
    assert_text(&app.world, "tide_message", "Voyage complete.");
    assert_text(
        &app.world,
        "tide_modal_best",
        &alloc::format!("BEST {total} · ★ New best"),
    );

    tap_id(&mut app.world, "tide_new_voyage");
    flush(&mut app.world);
    assert!(!model.complete());
    assert_eq!(model.turn(), 0);
    assert_eq!(model.modal(), TideModal::None);
    model.set_modal(TideModal::Voyage);
    flush(&mut app.world);
    assert_text(
        &app.world,
        "tide_chart_best",
        &alloc::format!("BEST {total}"),
    );
}

#[test]
fn a_whole_voyage_fits_in_chinese_too() {
    let (app, _) = sail(Locale::ZhCn);
    assert_text(&app.world, "tide_modal_title", "远航完成");
    assert_text(&app.world, "tide_message", "四岛航行完成。");
    assert_text(&app.world, "tide_new_voyage", "↻ 新的航行");
}

#[cfg(feature = "persistence")]
#[test]
fn persistent_actions_append_inside_the_model_update() {
    let world = fixture();
    let model = model_handle(&world);
    let index = first_valid(&world);

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
