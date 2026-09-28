use super::input::local_cell;
use super::setup_app;
use super::state::FactorySurface;
use crate::core::model::ModelHandle;
use crate::gallery::play::factory::{
    FactoryError, FactoryModel, FactoryModelHandle, FactoryTool, ModuleKind,
};
use crate::input::event::GestureHandler;
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
        with_model(&world, |model| local_cell(model, Point::new(183, 142))),
        Some(23)
    );
    assert_eq!(
        GestureHandler::trigger(
            &mut world,
            surface,
            &GestureEvent::Tap {
                x: Fixed::from_int(183),
                y: Fixed::from_int(142),
                target: surface,
            },
        ),
        Some(true)
    );
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
