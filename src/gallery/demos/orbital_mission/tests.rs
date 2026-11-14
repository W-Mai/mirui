use super::input::orbit_tick_system;
use super::setup_app;
use super::state::OrbitSurface;
use crate::core::model::ModelHandle;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::orbit::{
    OrbitError, OrbitModal, OrbitModel, OrbitModelHandle, OrbitPage, OrbitStatus,
};
use crate::prelude::App;
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

    assert_text(&world, "orbit_time", "SIM T+0.0");
    assert_text(&world, "orbit_mission", "任务 01 · 抬升轨道");
    assert_text(&world, "orbit_goal", "目标 1/1 · 外圈 113~130");
    assert!(texts_under(&world, "orbit_map_page").contains(&"0° 顺向".to_string()));
    assert!(shown(&world, "orbit_map_page"));

    model.flip_angle();
    model.set_page(OrbitPage::Plan);
    flush(&mut world);
    assert!(!shown(&world, "orbit_map_page"));
    assert!(shown(&world, "orbit_plan_page"));
    let plan = texts_under(&world, "orbit_plan_page");
    assert!(plan.contains(&"180° 反向".to_string()), "{plan:?}");
    assert!(plan.contains(&"3.8".to_string()), "{plan:?}");
    assert_text(&world, "orbit_queue_count", "节点  0 / 3");
    assert_text(&world, "orbit_next_node", "T+3.0");

    model.schedule().unwrap();
    flush(&mut world);
    assert_text(&world, "orbit_queue_count", "节点  1 / 3");
    assert_text(&world, "orbit_tab_plan", "计划 · 1");
    assert_text(&world, "orbit_plan_cost", "节点用燃料 7.6");
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
fn an_open_dialog_covers_the_pages_with_its_shade() {
    let mut world = fixture();
    let model = model_handle(&world);
    let shade = world.find_by_id("orbit_modal_shade").unwrap();
    assert!(world.has::<HitTarget>(shade));
    assert!(!shown(&world, "orbit_modal_shade"));

    model.set_modal(OrbitModal::Missions);
    flush(&mut world);
    assert!(shown(&world, "orbit_modal_shade"));
    assert_text(&world, "orbit_modal_title", "选择任务");
    assert_text(&world, "orbit_modal_count", "★ 0/8");
    assert!(!shown(&world, "orbit_modal_left"));

    model.choose_mission(6);
    flush(&mut world);
    assert_text(&world, "orbit_modal_title", "任务 07 · 失联之前");
    assert_text(&world, "orbit_modal_left", "提示");
    assert_text(&world, "orbit_contact", "4.0 s 后失联");
    model.toggle_hint();
    flush(&mut world);
    assert_text(
        &world,
        "orbit_brief_foot",
        "提示：先排节点：延时 10 s，Δv 3.4。再 Δv 3.8 点火。",
    );
}

#[test]
fn a_won_flight_shows_its_star_and_offers_the_next_mission() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.burn().unwrap();
    model.toggle_running();
    for _ in 0..1_000 {
        model.advance_ms(16);
        if model.status() != OrbitStatus::Running {
            break;
        }
    }
    flush(&mut world);
    assert_text(&world, "orbit_notice", "● 已到目标，可以采样");
    assert_text(&world, "orbit_footer_scan", "● 采样");
    model.scan().unwrap();
    flush(&mut world);
    assert_eq!(model.modal(), OrbitModal::Result);
    assert_text(&world, "orbit_modal_title", "01 抬升轨道 完成 ★");
    assert_text(&world, "orbit_modal_note", "★ 1/8   下一个：02 远点信标");
    assert_text(&world, "orbit_modal_right", "下一个任务 ▶");
    assert_text(&world, "orbit_mission", "任务 01 · 抬升轨道 ★");
    assert_text(&world, "orbit_footer_run", "下一个 ▶");

    model.next_mission();
    flush(&mut world);
    assert_text(&world, "orbit_modal_title", "任务 02 · 远点信标");
    assert_text(&world, "orbit_goal", "目标 1/1 · 信标 · 限速 24");
    assert!(!world.has::<Hidden>(world.find_by_id("orbit_modal").unwrap()));
}
