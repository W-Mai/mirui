use super::state::{Geometry, GlassModel};
use crate::prelude::*;

fn settle(model: &mut GlassModel) {
    for _ in 0..360 {
        model.advance(16);
    }
    assert!(!model.active());
}

#[test]
fn reversal_and_cancel_settle_using_the_shared_spring() {
    let mut model = GlassModel::default();
    model.toggle();
    for _ in 0..12 {
        model.advance(16);
    }
    assert!(model.scene().progress > Fixed::ZERO);
    model.toggle();
    for _ in 0..300 {
        model.advance(16);
    }
    assert!(!model.active());
    assert_eq!(model.scene().progress, Fixed::ZERO);
    model.pressed(true, true);
    model.begin_drag(Fixed::ZERO);
    model.drag(Fixed::from_int(140));
    assert!(model.scene().progress > Fixed::HALF);
    model.end_drag(Fixed::ZERO, true);
    for _ in 0..300 {
        model.advance(16);
    }
    assert!(!model.active());
    assert_eq!(model.scene().progress, Fixed::ZERO);
}

#[test]
fn held_drag_keeps_its_position_and_cancellation_restores_the_original_side() {
    let mut model = GlassModel::default();
    model.pressed(false, false);
    model.begin_drag(Fixed::ZERO);
    assert!(!model.is_dragging());
    model.drag(Fixed::from_int(140));
    assert_eq!(model.scene().progress, Fixed::ZERO);
    model.pressed(true, true);
    model.begin_drag(Fixed::ZERO);
    assert!(model.is_dragging());
    model.drag(Fixed::from_int(104));
    for _ in 0..180 {
        model.advance(16);
    }
    let held = model.scene();
    assert_eq!(held.progress, Fixed::HALF);
    model.advance(16);
    let next = model.scene();
    assert_eq!(next.progress, held.progress);
    for (a, b) in held.pixels.iter().zip(next.pixels) {
        assert_eq!(a.position, b.position);
    }
    model.end_drag(Fixed::from_int(400), true);
    settle(&mut model);
    assert_eq!(model.scene().progress, Fixed::ZERO);
    let revision = model.visual_revision();
    model.advance(16);
    assert_eq!(model.visual_revision(), revision);
}

#[test]
fn responsive_geometry_preserves_the_hit_area_and_knob_identity() {
    let model = GlassModel::default();
    for (width, height) in [(320, 320), (480, 320), (640, 480), (760, 520), (1200, 900)] {
        let g = Geometry::new(Rect::new(17, 23, width, height));
        assert!(g.contains(g.center.x, g.center.y));
        let knob = Point::new(Fixed::from_int(-104), Fixed::ZERO);
        assert!(model.knob_contains(knob));
        assert!(!model.knob_contains(Point::new(104, 0)));
        assert_eq!(g.local(g.center.x + knob.x * g.scale, g.center.y), knob);
        assert!(!g.contains(g.center.x + Fixed::from_int(217) * g.scale, g.center.y));
    }
}

#[test]
fn particle_animation_reuses_its_fixed_storage() {
    let mut model = GlassModel::default();
    let allocations = crate::test_support::tracked_allocations(|| {
        model.choose(true);
        settle(&mut model);
        model.choose(false);
        settle(&mut model);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn typed_pointer_delivery_animates_both_sides_and_dirty_paint_matches_full_paint() {
    use crate::input::event::input::{InputEvent, PointerKind};
    use crate::surface::FramebufferAccess;

    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let model = app.add_model(GlassModel::default()).finish();
    app.with_widget(super::glass_render::view());
    app.compose(root, super::build_widgets(model.clone()));
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    let initial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    for (time, event) in [
        (
            1,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(240),
                y: Fixed::from_int(230),
            },
        ),
        (
            20,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(240),
                y: Fixed::from_int(230),
            },
        ),
    ] {
        crate::input::event::deliver_input(&mut app.world, Some(root), &event, time, 640, 480);
    }
    for _ in 0..360 {
        model.advance(16);
    }
    assert_eq!(model.scene().progress, Fixed::ONE);
    assert!(!model.active());
    app.render_dirty().unwrap();
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_ne!(initial, dirty);
    app.render().unwrap();
    assert_eq!(
        dirty,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
    model.choose(false);
    for _ in 0..360 {
        model.advance(16);
    }
    app.render_dirty().unwrap();
    assert_eq!(model.scene().progress, Fixed::ZERO);
    assert_eq!(
        initial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}
