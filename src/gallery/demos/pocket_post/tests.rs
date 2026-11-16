use super::state::PostSurface;
use super::{VIEWPORT, setup_app};
use crate::core::i18n::{I18n, Locale};
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::post::{PostModel, PostModelHandle};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;

fn fixture(width: u16, height: u16) -> World {
    let mut app = App::headless(width, height);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&PostModel) -> R) -> R {
    let surface = world.find_by_id("post_surface").unwrap();
    let model = &world.get::<PostSurface>(surface).unwrap().model;
    crate::core::model::ModelHandle::read(model, inspect)
}

fn model_handle(world: &World) -> PostModelHandle {
    let surface = world.find_by_id("post_surface").unwrap();
    world.get::<PostSurface>(surface).unwrap().model.clone()
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn text_of(world: &World, id: &'static str) -> alloc::string::String {
    let entity = world.find_by_id(id).unwrap();
    let text = world.get::<Text>(entity).unwrap();
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
    text.resolve(world.resource()).as_ref().into()
}

fn assert_text(world: &World, id: &'static str, expected: &str) {
    assert_eq!(text_of(world, id), expected, "{id}");
}

fn has_text(world: &World, wanted: &str) -> bool {
    world.query::<Text>().collect().into_iter().any(|entity| {
        world
            .get::<Text>(entity)
            .is_some_and(|text| text.resolve(world.resource()).as_ref() == wanted)
    })
}

fn assert_bounded_text_valid(world: &World) {
    for id in [
        "post_shift",
        "post_sorted",
        "post_status",
        "post_station_0",
        "post_station_1",
        "post_station_2",
        "post_queue_0",
        "post_queue_1",
        "post_queue_2",
        "post_queue_3",
        "post_queue_4",
        "post_in_transit",
        "post_run",
        "post_speed",
        "post_modal_title",
        "post_modal_subtitle",
        "post_shift_stars",
        "post_summary_score",
        "post_summary_best",
        "post_summary_correct",
        "post_summary_missed",
        "post_summary_next",
        "post_reset_progress",
    ] {
        let entity = world.find_by_id(id).unwrap();
        let text = world.get::<Text>(entity).unwrap();
        assert!(text.capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

fn is_hidden(world: &World, id: &'static str) -> bool {
    world.has::<Hidden>(world.find_by_id(id).unwrap())
}

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    }
}

fn finish_the_shift(model: &PostModelHandle) {
    model.toggle_running();
    for _ in 0..2_000 {
        model.advance_ms(120);
        if model.finished() {
            break;
        }
    }
    assert!(model.finished());
}

#[test]
fn tick_system_uses_the_supplied_delta_and_preserves_zero() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    let model = app.add_model(PostModel::default()).finish();
    model.toggle_running();
    let distance = || {
        crate::core::model::ModelHandle::read(&model, |model| {
            model
                .active(0)
                .expect("running starts one parcel")
                .distance()
        })
    };
    let initial = distance();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(super::input::post_tick_system::system(model.clone()));

    app.world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut app.world).unwrap();
    assert_eq!(distance(), initial);

    app.world.insert_resource(DeltaTimeMs(16));
    scheduler.run_all(&mut app.world).unwrap();
    assert_eq!(distance(), initial);

    app.world.insert_resource(DeltaTimeMs(1));
    scheduler.run_all(&mut app.world).unwrap();
    assert!(distance() > initial);
}

#[test]
#[should_panic(expected = "requires resource")]
fn tick_system_rejects_a_missing_delta_resource() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    let model = app.add_model(PostModel::default()).finish();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(super::input::post_tick_system::system(model));

    scheduler.run_all(&mut app.world).unwrap();
}

#[test]
fn composition_uses_one_dense_route_surface_and_semantic_controls() {
    let world = fixture(VIEWPORT.0, VIEWPORT.1);
    assert!(world.find_by_id("post_surface").is_some());
    for id in [
        "post_run",
        "post_send",
        "post_speed",
        "post_restart",
        "post_shifts",
        "post_shift_stars",
    ] {
        assert!(world.find_by_id(id).is_some(), "{id}");
    }
    assert_eq!(world.query::<PostSurface>().collect().len(), 1);
}

#[test]
fn switch_buttons_sit_on_their_discs_and_flip_their_switch() {
    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    for (index, id) in ["post_switch_0", "post_switch_1"].into_iter().enumerate() {
        let button = world.find_by_id(id).unwrap();
        assert_eq!(with_model(&world, |model| model.switch(index)), 0);
        assert!(trigger(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: button,
            }
        ));
        assert_eq!(with_model(&world, |model| model.switch(index)), 1, "{id}");
    }
    model_handle(&world).open_shifts();
    flush(&mut world);
    assert!(crate::ui::branch::is_hidden_in_tree(
        &world,
        world.find_by_id("post_switch_0").unwrap()
    ));
}

#[test]
fn visual_changes_dirty_the_bound_surface_without_node_sync() {
    use crate::ui::dirty::VisualDirty;

    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let surface = world.find_by_id("post_surface").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);

    world
        .get::<PostSurface>(surface)
        .unwrap()
        .model
        .toggle_switch(0);
    crate::core::reactive::flush_signal_dirty(&mut world);

    assert!(world.has::<VisualDirty>(surface));
}

#[test]
fn observed_state_updates_bounded_text_and_visibility() {
    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let model = model_handle(&world);

    assert_text(&world, "post_status", "▶ to start · tap S1 S2 to switch");
    assert_text(&world, "post_shift", "01 Dawn");
    assert_text(&world, "post_run", "▶");
    assert!(is_hidden(&world, "post_modal"));
    assert_bounded_text_valid(&world);

    model.open_shifts();
    flush(&mut world);
    assert!(!is_hidden(&world, "post_modal"));
    assert!(!is_hidden(&world, "post_shift_list"));
    assert!(is_hidden(&world, "post_summary_score"));
    assert_text(&world, "post_modal_title", "Shifts");
    assert_text(
        &world,
        "post_modal_subtitle",
        "Sort every parcel right to earn ★.",
    );
    assert_text(&world, "post_shift_stars", "★ 0 / 6");
    assert!(has_text(&world, "01 Dawn\n6 parcels · every 3.0 s\n☆"));

    model.load_shift(1);
    flush(&mut world);
    assert!(is_hidden(&world, "post_modal"));
    assert_text(&world, "post_sorted", "Sorted 0/9");
    assert_text(&world, "post_shift", "02 Morning");

    model.load_shift(0);
    model.spawn_manual();
    model.spawn_manual();
    flush(&mut world);
    assert_text(&world, "post_status", "Paused · missed 0 · streak 0");
    assert_text(&world, "post_in_transit", "On rails 2/2");
    assert!(!is_hidden(&world, "post_queue_0"));
    assert!(is_hidden(&world, "post_queue_4"));

    model.open_reset();
    flush(&mut world);
    assert!(is_hidden(&world, "post_shift_list"));
    assert!(!is_hidden(&world, "post_reset_confirm"));
    assert_text(&world, "post_modal_title", "Restart this shift?");
    assert_text(&world, "post_reset_progress", "Dawn · sent 0/6 · 0 pts");
    assert_bounded_text_valid(&world);
}

#[test]
fn finishing_a_shift_offers_the_next_one() {
    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let model = model_handle(&world);
    finish_the_shift(&model);
    flush(&mut world);
    assert!(!is_hidden(&world, "post_summary_next"));
    assert_text(&world, "post_modal_title", "Dawn done");
    assert_text(
        &world,
        "post_modal_subtitle",
        "4 sent astray. Sort all right for ★.",
    );
    assert_text(&world, "post_summary_best", "New best 0220");
    assert_text(&world, "post_summary_correct", "Right 2");
    assert_text(&world, "post_summary_missed", "Astray 4");
    assert_text(&world, "post_summary_next", "Next: Morning →");
    assert_text(&world, "post_status", "Shift done · missed 4");
    assert_bounded_text_valid(&world);

    model.next_shift();
    flush(&mut world);
    assert!(is_hidden(&world, "post_modal"));
    assert_text(&world, "post_shift", "02 Morning");
    assert_text(&world, "post_status", "▶ to start · tap S1 S2 to switch");

    model.load_shift(5);
    model.open_shifts();
    model.close_modal();
    flush(&mut world);
    assert_text(&world, "post_summary_next", "☰ Shifts");
}

#[test]
fn every_word_follows_the_page_language() {
    let mut world = fixture(VIEWPORT.0, VIEWPORT.1);
    let model = model_handle(&world);
    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    flush(&mut world);
    assert_text(&world, "post_title", "口袋邮局");
    assert_text(&world, "post_shift", "01 晨班");
    assert_text(&world, "post_sorted", "送对 0/6");
    assert_text(&world, "post_status", "按 ▶ 开始 · 点 S1 S2 拨道岔");
    assert!(has_text(&world, "薄荷港") && has_text(&world, "入口") && has_text(&world, "待发"));

    finish_the_shift(&model);
    flush(&mut world);
    assert_text(&world, "post_modal_title", "晨班 完成");
    assert_text(&world, "post_modal_subtitle", "错投 4 件。全部送对才得 ★。");
    assert_text(&world, "post_summary_best", "新高 0220");
    assert_text(&world, "post_summary_next", "下一班 · 上午班 →");

    world.resource::<I18n>().unwrap().set_locale(Locale::EnUs);
    flush(&mut world);
    assert_text(&world, "post_modal_title", "Dawn done");
    assert_text(&world, "post_title", "POCKET POST");
}
