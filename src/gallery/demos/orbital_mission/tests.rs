use super::setup_app;
use super::state::OrbitSurface;
use crate::core::model::ModelHandle;
use crate::gallery::play::orbit::{OrbitError, OrbitModel, OrbitModelHandle, OrbitPage};
use crate::prelude::{App, World};
use crate::ui::Hidden;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;

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
    assert_eq!(text.resolve(world).as_ref(), expected, "{id}");
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
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
    assert!(!world.has::<Hidden>(world.find_by_id("orbit_map_page").unwrap()));

    model.set_page(OrbitPage::Plan);
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("orbit_map_page").unwrap()));
    assert!(!world.has::<Hidden>(world.find_by_id("orbit_plan_page").unwrap()));
    assert_text(&world, "orbit_queue_count", "0 / 3 个节点");

    for id in [
        "orbit_mission",
        "orbit_time",
        "orbit_fuel",
        "orbit_heat",
        "orbit_radius",
        "orbit_speed",
        "orbit_dv",
        "orbit_angle",
        "orbit_preview",
        "orbit_queue_count",
        "orbit_delay",
        "orbit_event",
        "orbit_modal_title",
        "orbit_modal_subtitle",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.text_capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}
