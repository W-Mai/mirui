use super::input::surface_gesture;
use super::setup_app;
use super::state::{FactoryNodes, FactorySurface};
use crate::gallery::play::factory::{FactoryModel, FactoryTool, ModuleKind};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("factory_surface").is_some());
    assert!(app.world.find_by_id("factory_footer_4").is_some());
    assert!(app.world.find_by_id("factory_reference").is_some());
    assert_eq!(app.world.query::<FactorySurface>().iter().count(), 1);
}

#[test]
fn surface_places_selected_tool_without_per_cell_widgets() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    let surface = app.world.find_by_id("factory_surface").unwrap();
    app.world
        .insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    FactoryNodes::update(&mut app.world, |model| {
        model.set_tool(FactoryTool::Build(ModuleKind::Belt))
    });
    assert!(surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::Tap {
            x: Fixed::from_int(183),
            y: Fixed::from_int(142),
            target: surface,
        }
    ));
    assert_eq!(
        app.world
            .resource::<FactoryModel>()
            .unwrap()
            .cell(23)
            .unwrap()
            .kind,
        ModuleKind::Belt
    );
}
