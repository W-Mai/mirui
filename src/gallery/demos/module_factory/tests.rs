use super::input::{factory_tick_system, local_cell};
use super::setup_app;
use super::state::FactorySurface;
use crate::core::model::ModelHandle;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::gallery::play::factory::{
    FactoryError, FactoryModel, FactoryModelHandle, FactoryTool, ModuleKind,
};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn model_handle(world: &World) -> FactoryModelHandle {
    let surface = world.find_by_id("factory_surface").unwrap();
    world.get::<FactorySurface>(surface).unwrap().model.clone()
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&FactoryModel) -> R) -> R {
    ModelHandle::read(&model_handle(world), inspect)
}

#[test]
fn bound_tick_uses_delta_time_and_zero_elapsed_is_idle() {
    let mut world = fixture();
    let model = model_handle(&world);
    model.toggle_run().unwrap();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(factory_tick_system::system(model.clone()));

    world.insert_resource(DeltaTimeMs(1_000));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(model.tick(), 2);

    let revision = model.visual_revision();
    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world).unwrap();
    assert_eq!(model.tick(), 2);
    assert_eq!(model.visual_revision(), revision);
}

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let world = fixture();
    assert!(world.find_by_id("factory_surface").is_some());
    assert!(world.find_by_id("factory_footer_4").is_some());
    assert!(world.find_by_id("factory_reference").is_some());
    assert_eq!(world.query::<FactorySurface>().collect().len(), 1);
}

#[test]
fn surface_places_selected_tool_through_the_bound_model() {
    let mut world = fixture();
    let surface = world.find_by_id("factory_surface").unwrap();
    let rect = Rect::new(0, 0, 480, 320);
    world.insert(surface, ComputedRect(rect));
    let model = model_handle(&world);
    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    assert_eq!(
        with_model(&world, |model| {
            local_cell(model.page(), model.modal(), Point::new(183, 142))
        }),
        Some(23)
    );
    assert!({
        let decision = crate::__macro_support::dispatch_entity_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::from_int(183),
                y: Fixed::from_int(142),
                target: surface,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    });
    assert_eq!(
        with_model(&world, |model| model.cell(23).unwrap().kind),
        ModuleKind::Belt
    );
}

#[test]
fn fallible_commands_publish_only_after_success() {
    let mut world = fixture();
    let surface = world.find_by_id("factory_surface").unwrap();
    let model = model_handle(&world);
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(surface);

    assert_eq!(model.rotate_active(), Err(FactoryError::NothingSelected));
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!world.has::<VisualDirty>(surface));

    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    crate::core::reactive::flush_signal_dirty(&mut world);
    world.remove::<VisualDirty>(surface);
    assert_eq!(
        model.rotate_active(),
        Ok(crate::gallery::play::change::ChangeSet::MODEL
            | crate::gallery::play::change::ChangeSet::VISUAL)
    );
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(world.has::<VisualDirty>(surface));
    assert_eq!(with_model(&world, FactoryModel::tool_direction), 1);
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
fn every_page_order_and_dialog_reads_cleanly() {
    use crate::gallery::play::factory::{FactoryModal, FactoryPage, FactoryStatus, MISSION_COUNT};

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.render().unwrap();
    let model = model_handle(&app.world);
    for mission in 0..MISSION_COUNT as u8 {
        model.request_mission(mission, true);
        for page in [
            FactoryPage::Line,
            FactoryPage::Orders,
            FactoryPage::Telemetry,
        ] {
            model.set_page(page);
            settle(&mut app, false);
        }
        model.set_page(FactoryPage::Line);
        loop {
            model.step_once().unwrap();
            if matches!(model.status(), FactoryStatus::Won | FactoryStatus::Timeout) {
                break;
            }
            if model.tick() % 16 == 0 {
                settle(&mut app, false);
            }
        }
        settle(&mut app, false);
        model.set_page(FactoryPage::Telemetry);
        settle(&mut app, false);
        model.set_page(FactoryPage::Orders);
        settle(&mut app, false);
    }
    model.set_page(FactoryPage::Line);
    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    model.tap_cell(0);
    settle(&mut app, false);
    for modal in [FactoryModal::Tools, FactoryModal::Help] {
        model.open_modal(modal);
        settle(&mut app, true);
        model.close_modal();
    }
    model.request_mission(0, false);
    settle(&mut app, true);
    model.close_modal();
    model.request_mission(0, true);
    settle(&mut app, true);
}

/// Taps whatever control sits at `(x, y)` on the rendered screen.
fn tap_at<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, root: Entity, x: i32, y: i32) {
    let (x, y) = (Fixed::from_int(x), Fixed::from_int(y));
    let target = crate::input::event::hit_test::hit_test(&app.world, root, x, y, 480, 320).unwrap();
    crate::__macro_support::dispatch_entity_gesture(
        &mut app.world,
        &GestureEvent::Tap { x, y, target },
        0,
    )
    .unwrap();
    crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
    app.render_dirty().unwrap();
}

#[test]
fn order_tiles_browse_and_the_start_button_loads() {
    use crate::gallery::play::factory::{FactoryPage, MISSIONS};

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.render().unwrap();
    let model = model_handle(&app.world);
    tap_at(&mut app, root, 148, 45);
    assert_eq!(model.page(), FactoryPage::Orders);
    tap_at(&mut app, root, 227, 136);
    assert_eq!(model.browse(), 3);
    assert_eq!(model.mission_index(), 0);
    tap_at(&mut app, root, 353, 230);
    assert_eq!(model.mission_index(), 3);
    assert_eq!(model.page(), FactoryPage::Line);
    assert_eq!(model.selected(), usize::from(MISSIONS[3].focus));
    assert!(with_model(&app.world, |model| {
        (0..crate::gallery::play::factory::CELL_COUNT)
            .all(|index| model.cell(index) == MISSIONS[3].start[index])
    }));
}
