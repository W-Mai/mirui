use super::VIEWPORT;
use super::geometry::{TEXT_WINDOW, WAVE};
use super::runtime::{compact_curve_animation_system, install};
use super::style::BACKGROUND;
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::{DeltaTimeMs, SystemScheduler, World};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::render::path::PathCmd;
use crate::surface::FramebufferAccess;

fn run_compact_animation(world: &mut World) {
    let mut scheduler = SystemScheduler::new();
    scheduler.add(compact_curve_animation_system::system());
    scheduler.run_all(world).unwrap();
}

#[test]
fn animation_uses_the_current_text_path() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    let text = app.world.find_by_id("compact_curve_text_primary").unwrap();
    assert!(app.world.has::<super::state::CompactCurveMotion>(text));

    static REPLACEMENT: Path = path!("M 0 0 L 600 0");
    let replacement = app.paths().insert_static(REPLACEMENT.commands()).unwrap();
    let initial = super::geometry::text_path(replacement, Fixed::ZERO);
    app.world.insert(text, initial);
    app.add_resource(DeltaTimeMs(16));

    let mut scheduler = SystemScheduler::new();
    scheduler.add(compact_curve_animation_system::system());
    scheduler.run_all(&mut app.world).unwrap();
    let current = *app.world.get::<crate::text::TextPath>(text).unwrap();
    assert_eq!(current.path(), replacement);
    assert!(current.offset() > initial.offset());
    assert_eq!(
        current.end().unwrap() - current.offset(),
        Fixed::from_int(TEXT_WINDOW)
    );

    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..32 {
            scheduler.run_all(&mut app.world).unwrap();
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn attached_motion_is_independent_and_does_not_animate_plain_paths() {
    use super::state::CompactCurveMotion;
    use crate::text::TextPath;
    use crate::ui::widgets::Text;

    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let path = app.paths().insert_static(WAVE.commands()).unwrap();
    let (first, plain) = app.compose(root, |tree| {
        (
            tree.spawn((
                Text::label("First"),
                TextPath::new(path),
                CompactCurveMotion::default(),
            )),
            tree.spawn((Text::label("Plain"), TextPath::new(path))),
        )
    });
    app.add_resource(DeltaTimeMs(16));
    run_compact_animation(&mut app.world);
    let second = app.compose(root, |tree| {
        tree.spawn((
            Text::label("Second"),
            TextPath::new(path),
            CompactCurveMotion::default(),
        ))
    });
    run_compact_animation(&mut app.world);

    let first = app.world.get::<TextPath>(first).unwrap();
    let second = app.world.get::<TextPath>(second).unwrap();
    assert!(first.offset() > second.offset());
    assert!(second.offset() > Fixed::ZERO);
    assert_eq!(
        *app.world.get::<TextPath>(plain).unwrap(),
        TextPath::new(path)
    );
}

#[test]
fn wave_has_fixed_topology_and_alternating_extrema() {
    let path = &WAVE;
    assert_eq!(path.commands().len(), 9);
    assert!(path.is_borrowed());
    let PathCmd::MoveTo(start) = path.commands()[0] else {
        panic!("compact path start");
    };
    let PathCmd::CubicTo { ctrl1: crest, .. } = path.commands()[1] else {
        panic!("compact path crest");
    };
    let PathCmd::CubicTo { ctrl1: trough, .. } = path.commands()[2] else {
        panic!("compact path trough");
    };
    assert_eq!(start.x, Fixed::from_int(-160));
    assert_eq!(crest.y, Fixed::from_int(10));
    assert_eq!(trough.y, Fixed::from_int(66));
}

#[test]
fn embedded_layout_uses_bitmap_text_and_constant_path_window() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    flush_signal_dirty(&mut app.world);
    app.render().unwrap();

    let text = app.world.find_by_id("compact_curve_text_primary").unwrap();
    let initial = *app.world.get::<crate::text::TextPath>(text).unwrap();
    assert_eq!(
        app.world
            .get::<crate::ui::Style>(text)
            .unwrap()
            .font_stack
            .primary(),
        &FontToken::Default
    );

    app.world.insert_resource(DeltaTimeMs(16));
    run_compact_animation(&mut app.world);
    let current = *app.world.get::<crate::text::TextPath>(text).unwrap();
    assert!(current.offset() > initial.offset());
    assert_eq!(
        current.end().unwrap() - current.offset(),
        Fixed::from_int(TEXT_WINDOW)
    );
    assert!(!app.backend.framebuffer().buf.as_slice().is_empty());
}

#[test]
fn compact_shell_resolves_the_active_theme() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    app.set_theme(crate::gallery::showcase_theme::LIGHT_ID)
        .unwrap();
    flush_signal_dirty(&mut app.world);
    app.render().unwrap();

    let surface = crate::gallery::showcase_theme::light().resolve(BACKGROUND);
    assert_eq!(
        &app.backend.framebuffer().buf.as_slice()[..3],
        &[surface.r, surface.g, surface.b]
    );
}

#[test]
#[should_panic(expected = "requires resource")]
fn compact_curve_motion_requires_frame_delta() {
    run_compact_animation(&mut World::new());
}
