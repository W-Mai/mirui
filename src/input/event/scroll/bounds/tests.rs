use super::*;
use crate::prelude::*;
use crate::test_support::tracked_allocations;
use crate::types::Viewport;
use crate::ui::render_system::{prepare_input_geometry, try_update_layout};
use crate::ui::{Children, ComputedRect};

#[crate::compose]
fn panel() -> Entity {
    ui! {
        Column(width: 80, height: 60, clip_children: true) [
            ScrollOffset::default(), ScrollConfig::default(),
        ] {
            View(width: 120, height: 150, shrink: 0.0)
        }
    }
}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn setup() -> (TestApp, Entity, Entity) {
    let mut app = App::headless(160, 160);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, panel());
    (app, root, list)
}

#[test]
fn successful_layout_publishes_changed_bounds_without_hot_allocations() {
    let (mut app, root, list) = setup();
    assert!(!app.world.has::<ScrollBounds>(list));
    app.world.watch_component_revision::<ScrollBounds>();
    app.render().unwrap();
    let initial = *app.world.get::<ScrollBounds>(list).unwrap();
    assert_eq!(initial, scroll_bounds(&app.world, list).unwrap());
    assert_eq!((initial.max_x, initial.max_y), (40.into(), 90.into()));
    assert!(!app.world.has::<ScrollBounds>(root));
    let viewport = Viewport::new(160, 160, Fixed::ONE);
    let revision = app.world.component_revision::<ScrollBounds>();
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(revision, app.world.component_revision::<ScrollBounds>());
    assert_eq!(
        tracked_allocations(|| {
            for height in [200, 220, 180] {
                app.world
                    .get_mut::<ScrollConfig>(list)
                    .unwrap()
                    .content_height = height.into();
                assert_eq!(
                    app.world.get::<ScrollBounds>(list).unwrap().viewport_height,
                    60.into()
                );
                prepare_input_geometry(&mut app.world, root, &viewport).unwrap();
                assert_eq!(
                    app.world.get::<ScrollBounds>(list).unwrap().max_y,
                    (height - 60).into()
                );
            }
        }),
        0
    );
}

#[test]
fn config_changes_refresh_idle_dirty_render_and_input_geometry() {
    let (mut app, root, list) = setup();
    app.render().unwrap();
    app.render_dirty().unwrap();
    app.world
        .get_mut::<ScrollConfig>(list)
        .unwrap()
        .content_height = 280.into();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        90.into()
    );
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        220.into()
    );
    app.world.remove::<ScrollConfig>(list);
    prepare_input_geometry(&mut app.world, root, &Viewport::new(160, 160, Fixed::ONE)).unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        90.into()
    );
}

#[test]
fn detached_or_removed_scrollers_lose_published_bounds() {
    let (mut app, root, list) = setup();
    let viewport = Viewport::new(160, 160, Fixed::ONE);
    app.render().unwrap();
    app.world.remove::<ScrollOffset>(list);
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert!(!app.world.has::<ScrollBounds>(list));
    app.world.insert(list, ScrollOffset::default());
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert!(app.world.has::<ScrollBounds>(list));
    app.world.get_mut::<Children>(root).unwrap().0.clear();
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert!(!app.world.has::<ScrollBounds>(list));
}

#[test]
fn clipping_and_resizing_share_the_controller_extent_algorithm() {
    let (mut app, root, list) = setup();
    let child = app.world.get::<Children>(list).unwrap().0[0];
    app.compose(child, |cx| {
        ui! { View(width: 200, height: 300, shrink: 0.0) }
    });
    app.render().unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        240.into()
    );
    app.world.get_mut::<Style>(child).unwrap().clip_children = true;
    try_update_layout(&mut app.world, root, &Viewport::new(160, 160, Fixed::ONE)).unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        90.into()
    );
    app.world.get_mut::<Style>(list).unwrap().layout.height = Dimension::px(100);
    try_update_layout(&mut app.world, root, &Viewport::new(160, 160, Fixed::ONE)).unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        50.into()
    );
    assert_eq!(app.world.get::<ComputedRect>(list).unwrap().0.h, 100.into());
}

#[test]
fn failed_layout_keeps_last_published_scroll_geometry() {
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_input::Placeholder;
    let (mut app, root, list) = setup();
    app.with_text_layout_limits(crate::text::TextLayoutLimits {
        text_bytes: 2,
        ..crate::text::TextLayoutLimits::default()
    });
    let input = app.compose(root, |cx| {
        ui! { TextInput(width: 64, height: 20) }
    });
    app.render().unwrap();
    let before = *app.world.get::<ScrollBounds>(list).unwrap();
    app.world
        .get_mut::<ScrollConfig>(list)
        .unwrap()
        .content_height = 240.into();
    app.world.insert(input, Placeholder("Type"));
    assert!(app.render().is_err());
    assert_eq!(app.world.get::<ScrollBounds>(list), Some(&before));
    app.world.remove::<Placeholder>(input);
    app.render().unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        180.into()
    );
}
