use super::input::orbit_tick_system;
use super::setup_app;
use super::state::OrbitSurface;
use crate::core::i18n::{I18n, Locale};
use crate::core::model::ModelHandle;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::orbit::{
    OrbitError, OrbitModal, OrbitModel, OrbitModelHandle, OrbitPage, OrbitStatus,
};
use crate::input::event::dispatch_input;
use crate::input::event::input::KeyModifiers;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, Hidden, HitTarget};

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn model_handle(world: &World) -> OrbitModelHandle {
    let surface = world.find_by_id("orbit_surface").unwrap();
    world.get::<OrbitSurface>(surface).unwrap().model.clone()
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&OrbitModel) -> R) -> R {
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

fn shown(world: &World, id: &'static str) -> bool {
    !crate::ui::branch::is_hidden_in_tree(world, world.find_by_id(id).unwrap())
}

/// The text of every Text under the named node, in tree order.
fn texts_under(world: &World, id: &'static str) -> Vec<String> {
    let mut found = Vec::new();
    let mut queue = vec![world.find_by_id(id).unwrap()];
    while let Some(entity) = queue.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            found.push(text.resolve(world.resource()).to_string());
        }
        if let Some(children) = world.get::<Children>(entity) {
            queue.extend(children.0.iter().rev().copied());
        }
    }
    found
}

/// Flies mission 1's burn until the craft reaches the band and pauses.
fn reach_the_first_goal(model: &OrbitModelHandle) {
    model.burn().unwrap();
    model.toggle_running();
    for _ in 0..1_000 {
        model.advance_ms(16);
        if model.status() != OrbitStatus::Running {
            break;
        }
    }
}

#[test]
fn bound_tick_uses_delta_time_and_zero_elapsed_is_idle() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.toggle_running();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(orbit_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(100));
    scheduler.run_all(&mut world).unwrap();
    let time = with_model(&world, OrbitModel::time);
    assert!(time.is_positive());

    let revision = model.visual_revision();
    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(with_model(&world, OrbitModel::time), time);
    assert_eq!(model.visual_revision(), revision);
}

#[test]
fn composition_binds_one_model_to_surface_and_modal() {
    let world = fixture();
    assert!(world.find_by_id("orbit_surface").is_some());
    assert!(world.find_by_id("orbit_modal").is_some());
    assert!(world.find_by_id("orbit_footer_burn").is_some());
    assert_eq!(world.query::<OrbitSurface>().collect().len(), 1);
}

#[test]
fn rejected_commands_do_not_publish_visual_changes() {
    let mut world = fixture();
    let surface = world.find_by_id("orbit_surface").unwrap();
    let model = model_handle(&world);
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);
    let revision = model.visual_revision();

    assert_eq!(model.cancel_at(0), Err(OrbitError::MissingNode));
    assert_eq!(model.scan(), Err(OrbitError::OutsideWindow));
    model.press_scan();
    assert_eq!(model.visual_revision(), revision);
    flush(&mut world);
    assert!(!world.has::<VisualDirty>(surface));

    model.adjust_dv(2);
    flush(&mut world);
    assert!(world.has::<VisualDirty>(surface));
    assert_eq!(with_model(&world, OrbitModel::dv_tenths), 40);
}

#[test]
fn observed_state_drives_bounded_text_and_page_visibility() {
    let mut world = fixture();
    let model = model_handle(&world);

    assert_text(&world, "orbit_time", "T+0.0");
    assert_text(&world, "orbit_mission", "01 Raise orbit");
    assert_text(&world, "orbit_goal", "Goal 1/1 · Outer 113~130");
    assert_text(&world, "orbit_dv", "Δv 3.8");
    assert_text(&world, "orbit_angle", "0°");
    assert!(shown(&world, "orbit_map_page"));
    assert!(shown(&world, "orbit_footer_burn"));
    assert!(!shown(&world, "orbit_footer_schedule"));

    model.flip_angle();
    model.set_page(OrbitPage::Plan);
    flush(&mut world);
    assert!(!shown(&world, "orbit_map_page"));
    assert!(shown(&world, "orbit_plan_page"));
    assert!(!shown(&world, "orbit_footer_burn"));
    assert!(shown(&world, "orbit_footer_schedule"));
    assert_text(&world, "orbit_angle", "180°");
    assert_text(&world, "orbit_queue_count", "NODES  0 / 3");
    assert_text(&world, "orbit_next_node", "T+3.0");
    assert_text(&world, "orbit_footer_schedule", "＋ Node · T+3.0");

    model.press_schedule();
    flush(&mut world);
    assert_text(&world, "orbit_queue_count", "NODES  1 / 3");
    assert_text(&world, "orbit_tab_plan", "PLAN · 1");
    assert_text(&world, "orbit_plan_cost", "NODE FUEL 7.6");
    assert!(
        texts_under(&world, "orbit_plan_page").contains(&"01  T+3.0  Δv 3.8  180°".to_string())
    );

    for id in [
        "orbit_mission",
        "orbit_time",
        "orbit_goal",
        "orbit_notice",
        "orbit_contact",
        "orbit_fuel",
        "orbit_heat",
        "orbit_radius",
        "orbit_speed",
        "orbit_preview",
        "orbit_dv",
        "orbit_angle",
        "orbit_queue_count",
        "orbit_delay",
        "orbit_next_node",
        "orbit_plan_cost",
        "orbit_status",
        "orbit_modal_title",
        "orbit_modal_body",
        "orbit_modal_note",
        "orbit_brief_foot",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn every_label_follows_the_page_language() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.set_modal(OrbitModal::Brief);
    flush(&mut world);
    assert_text(&world, "orbit_tab_map", "MAP");
    assert_text(&world, "orbit_modal_right", "Start");

    world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
    flush(&mut world);
    assert_text(&world, "orbit_tab_map", "航图");
    assert_text(&world, "orbit_tab_plan", "计划");
    assert_text(&world, "orbit_mission", "01 抬升轨道");
    assert_text(&world, "orbit_goal", "目标 1/1 · 外圈 113~130");
    assert_text(&world, "orbit_fuel", "燃料 48.0");
    assert_text(&world, "orbit_footer_burn", "点火");
    assert_text(&world, "orbit_modal_title", "任务 01 · 抬升轨道");
    assert_text(&world, "orbit_modal_right", "开始");
    assert_text(
        &world,
        "orbit_modal_body",
        "顺着飞行方向点火，轨道另一侧会升高。飞进橙色带时采样。",
    );
}

#[test]
fn an_open_dialog_covers_the_pages_with_its_shade() {
    let mut world = fixture();
    let model = model_handle(&world);
    let shade = world.find_by_id("orbit_modal_shade").unwrap();
    assert!(world.has::<HitTarget>(shade));
    assert!(!shown(&world, "orbit_modal_shade"));

    model.set_modal(OrbitModal::Missions);
    flush(&mut world);
    assert!(shown(&world, "orbit_modal_shade"));
    assert_text(&world, "orbit_modal_title", "Orbital Mission");
    assert_text(&world, "orbit_modal_count", "★ 0/8");
    assert!(!shown(&world, "orbit_modal_left"));

    model.choose_mission(6);
    flush(&mut world);
    assert_text(&world, "orbit_modal_title", "Mission 07 · Blackout");
    assert_text(&world, "orbit_modal_left", "Hint");
    assert_text(&world, "orbit_contact", "Contact lost in 4.0 s");
    model.modal_secondary();
    flush(&mut world);
    assert_text(&world, "orbit_modal_left", "Hide hint");
    assert_text(
        &world,
        "orbit_brief_foot",
        "Hint: Plan a node first: delay 10 s, Δv 3.4. Then burn Δv 3.8.",
    );
}

#[test]
fn a_won_flight_shows_its_star_and_offers_the_next_mission() {
    let mut world = fixture();
    let model = model_handle(&world);
    reach_the_first_goal(&model);
    flush(&mut world);
    assert_text(&world, "orbit_notice", "● In range · scan now");
    assert_text(&world, "orbit_footer_scan", "● Scan");
    model.press_scan();
    flush(&mut world);
    assert_eq!(model.modal(), OrbitModal::Result);
    assert_text(&world, "orbit_modal_title", "01 Raise orbit complete ★");
    assert_text(&world, "orbit_modal_note", "★ 1/8   Next: 02 Far beacon");
    assert_text(&world, "orbit_modal_right", "Next mission ▶");
    assert_text(&world, "orbit_mission", "01 Raise orbit ★");
    assert_text(&world, "orbit_footer_run", "▶");

    model.modal_primary();
    flush(&mut world);
    assert_text(&world, "orbit_modal_title", "Mission 02 · Far beacon");
    assert_text(&world, "orbit_goal", "Goal 1/1 · Beacon · max 24");
    assert!(!world.has::<Hidden>(world.find_by_id("orbit_modal").unwrap()));
}

#[test]
fn a_refused_tap_says_why_until_it_runs_out() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.press_scan();
    flush(&mut world);
    assert_text(&world, "orbit_notice", "Not at the goal yet");
    model.press_burn();
    flush(&mut world);
    assert_text(&world, "orbit_notice", "");
    for _ in 0..3 {
        model.press_burn();
    }
    model.adjust_dv(80);
    model.press_burn();
    flush(&mut world);
    assert_text(&world, "orbit_footer_burn", "Too hot");
    assert_text(&world, "orbit_notice", "Too hot · let it cool");
    model.advance_ms(1_000);
    model.advance_ms(1_000);
    flush(&mut world);
    assert_text(&world, "orbit_notice", "");
}

#[test]
fn keys_drive_the_dial_the_flight_and_the_dialog() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    let model = model_handle(&app.world);
    let mut press = |key: PhysicalKey, shift: bool| {
        let mut event = crate::test_support::physical_key(key);
        if shift {
            event.modifiers = Some(KeyModifiers {
                shift: true,
                ..Default::default()
            });
        }
        dispatch_input(&mut app.world, None, &event.into(), 0, 480, 320);
    };
    press(PhysicalKey::ArrowUp, false);
    assert_eq!(model.dv_tenths(), 40);
    press(PhysicalKey::ArrowUp, true);
    assert_eq!(model.dv_tenths(), 50);
    press(PhysicalKey::ArrowDown, true);
    press(PhysicalKey::ArrowDown, false);
    assert_eq!(model.dv_tenths(), 38);
    press(PhysicalKey::ArrowRight, false);
    assert_eq!(model.angle_degrees(), 15);
    press(PhysicalKey::KeyF, false);
    assert_eq!(model.angle_degrees(), -165);
    press(PhysicalKey::ArrowLeft, false);
    press(PhysicalKey::KeyF, false);
    assert_eq!(model.angle_degrees(), 0);
    press(PhysicalKey::KeyB, false);
    assert!(model.fuel() < crate::types::Fixed64::from_int(48));
    press(PhysicalKey::Space, false);
    assert_eq!(model.status(), OrbitStatus::Running);
    press(PhysicalKey::KeyH, false);
    assert_eq!(model.modal(), OrbitModal::Brief);
    press(PhysicalKey::KeyB, false);
    assert_eq!(model.modal(), OrbitModal::Brief);
    press(PhysicalKey::Enter, false);
    assert_eq!(model.modal(), OrbitModal::None);
    press(PhysicalKey::Digit2, false);
    assert_eq!(model.page(), OrbitPage::Plan);
}
