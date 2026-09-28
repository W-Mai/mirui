use super::input::surface_gesture;
use super::setup_app;
use super::state::CircuitSurface;
use crate::gallery::play::circuit::CircuitModel;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{App, Fixed, Rect};
use crate::ui::ComputedRect;

#[test]
fn composition_uses_one_dense_surface_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("circuit_surface").is_some());
    assert!(app.world.find_by_id("circuit_footer_4").is_some());
    assert!(app.world.find_by_id("circuit_task").is_some());
    assert_eq!(app.world.query::<CircuitSurface>().iter().count(), 1);
}

#[test]
fn gate_drag_cancel_leaves_authoritative_position_unchanged() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    let surface = app.world.find_by_id("circuit_surface").unwrap();
    app.world
        .insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let before = app
        .world
        .resource::<CircuitModel>()
        .unwrap()
        .gate_by_id(1)
        .unwrap();
    assert!(surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(145),
            y: Fixed::from_int(150),
            target: surface
        }
    ));
    assert!(surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragMove {
            x: Fixed::from_int(180),
            y: Fixed::from_int(210),
            dx: Fixed::from_int(35),
            dy: Fixed::from_int(60),
            target: surface
        }
    ));
    assert!(surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(180),
            y: Fixed::from_int(210),
            target: surface
        }
    ));
    assert_eq!(
        app.world
            .resource::<CircuitModel>()
            .unwrap()
            .gate_by_id(1)
            .unwrap(),
        before
    );
}
