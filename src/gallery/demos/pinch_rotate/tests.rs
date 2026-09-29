use super::runtime::setup_app;
use super::state::{PinchStatus, PinchTarget};
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::types::Fixed64;
use crate::ui::Children;
use crate::ui::widgets::{ParagraphStyle, Text, WidgetTransform};

type TestApp = App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
    crate::app::SwRendererFactory,
>;

fn fixture(width: u16, height: u16) -> TestApp {
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app
}

#[test]
fn build_widgets_smoke() {
    let app = fixture(super::DEFAULT_VIEW.0, super::DEFAULT_VIEW.1);
    let parent = app.root.expect("root");
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn pinch_updates_target_and_status() {
    let mut app = fixture(super::DEFAULT_VIEW.0, super::DEFAULT_VIEW.1);
    let target = app.world.find_by_id("pinch_target").expect("target id");
    let status = app.world.find_by_id("pinch_status").expect("status id");
    let model = app.world.get::<PinchTarget>(target).unwrap().model.clone();

    assert_eq!(model.status().pinch_events, 0);
    GestureHandler::trigger(
        &mut app.world,
        target,
        &GestureEvent::Pinch {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            scale_delta: Fixed64::from_int(2),
            target,
        },
    );
    assert_eq!(model.status().pinch_events, 1);
    assert_eq!(model.status().mode, "EXPAND");
    flush_signal_dirty(&mut app.world);
    assert!(
        app.world
            .get::<Text>(status)
            .expect("status text")
            .resolve(&app.world)
            .contains("EXPAND")
    );
    assert_eq!(
        app.world
            .get::<WidgetTransform>(target)
            .map(|value| value.0),
        Some(model.transform())
    );
    assert_eq!(
        app.world.get::<Text>(status).map(Text::paragraph),
        Some(&ParagraphStyle::label())
    );
    assert!(
        app.world
            .get::<Text>(status)
            .is_some_and(|text| text.text_capacity() == Some(64))
    );
}

#[test]
fn portrait_layout_reflows_the_target_between_status_and_footer() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let mut app = fixture(360, 480);
    let root = app.root.expect("root");
    update_layout(&mut app.world, root, &Viewport::new(360, 480, Fixed::ONE));

    let target = app.world.find_by_id("pinch_target").expect("target id");
    let status = app.world.find_by_id("pinch_status").expect("status id");
    let target_rect = app
        .world
        .get::<ComputedRect>(target)
        .expect("target rect")
        .0;
    let status_rect = app
        .world
        .get::<ComputedRect>(status)
        .expect("status rect")
        .0;

    assert_eq!(status_rect.x.to_int(), 16);
    assert_eq!(status_rect.w.to_int(), 328);
    assert_eq!(target_rect.x.to_int(), 100);
    assert!(target_rect.y > status_rect.y + status_rect.h);
    assert!(target_rect.y + target_rect.h < Fixed::from_int(444));
}

#[test]
fn portrait_status_uses_the_compact_vocabulary() {
    let status = PinchStatus {
        mode: "ROTATE",
        scale_pct: 158,
        rotation_deg: 25,
        pinch_events: 791,
        rotate_events: 3,
    };

    assert_eq!(
        alloc::format!("{status}"),
        "ROTATE · 158% · 25 DEG · P791 R3"
    );
}
