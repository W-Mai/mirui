use super::input::{local_cell, surface_gesture};
use super::setup_app;
use super::state::MossSurface;
use crate::gallery::play::moss::MossModel;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{App, Fixed};

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("moss_surface").is_some());
    assert!(app.world.find_by_id("moss_run").is_some());
    assert!(app.world.find_by_id("moss_seed_2").is_some());
    assert_eq!(app.world.query::<MossSurface>().iter().count(), 1);
}

#[test]
fn cancelled_drag_restores_the_whole_garden_transaction() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    let surface = app.world.find_by_id("moss_surface").unwrap();
    assert_eq!(
        local_cell(
            &app.world,
            surface,
            Fixed::from_int(16),
            Fixed::from_int(67)
        ),
        None
    );
    let before = *app.world.resource::<MossModel>().unwrap().cells();
    surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(18),
            y: Fixed::from_int(68),
            target: surface,
        },
    );
    surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragMove {
            x: Fixed::from_int(350),
            y: Fixed::from_int(265),
            dx: Fixed::from_int(332),
            dy: Fixed::from_int(197),
            target: surface,
        },
    );
    surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(350),
            y: Fixed::from_int(265),
            target: surface,
        },
    );
    assert_eq!(*app.world.resource::<MossModel>().unwrap().cells(), before);
}
