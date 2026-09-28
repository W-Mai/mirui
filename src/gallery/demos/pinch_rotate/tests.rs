use super::composition::build_widgets;
use super::state::{PinchStatus, PinchTarget};
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::types::Fixed64;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::widgets::{ParagraphStyle, Text, WidgetTransform};

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn pinch_updates_target_and_status() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    let target = world.find_by_id("pinch_target").expect("target id");
    let status = world.find_by_id("pinch_status").expect("status id");

    assert_eq!(
        world.get::<PinchTarget>(target).map(|t| t.pinch_events),
        Some(0)
    );
    GestureHandler::trigger(
        &mut world,
        target,
        &GestureEvent::Pinch {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            scale_delta: Fixed64::from_int(2),
            target,
        },
    );
    assert_eq!(
        world.get::<PinchTarget>(target).map(|t| t.pinch_events),
        Some(1)
    );
    assert_eq!(
        world.get::<PinchTarget>(target).map(|t| t.mode),
        Some("EXPAND")
    );
    flush_signal_dirty(&mut world);
    assert!(
        world
            .get::<Text>(status)
            .expect("status text")
            .resolve(&world)
            .contains("EXPAND")
    );
    assert!(world.has::<WidgetTransform>(target));
    assert_eq!(
        world.get::<Text>(status).map(Text::paragraph),
        Some(&ParagraphStyle::label())
    );
}

#[test]
fn portrait_layout_reflows_the_target_between_status_and_footer() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let mut app = App::headless(360, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets);
    app.set_root(root);
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
    let label = PinchStatus {
        mode: "ROTATE",
        scale_pct: 158,
        rotation_deg: 25,
        pinch_events: 791,
        rotate_events: 3,
    }
    .label();

    assert_eq!(label, "ROTATE · 158% · 25 DEG · P791 R3");
}
