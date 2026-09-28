use super::DEFAULT_VIEW;
use super::animation::BlurPan;
use super::composition::build_widgets;
use super::runtime::setup_app;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::render_system::update_layout;
use crate::ui::widgets::WidgetTransform;
use crate::ui::{Children, ComputedRect, IdMap, Theme, UiScope};

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn cards_stay_inside_portrait_landscape_and_desktop_viewports() {
    for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets);
        app.set_root(root);
        update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        for id in [
            "effect_mirror_card",
            "effect_temporal_card",
            "effect_blur_card",
            "effect_light_card",
        ] {
            let entity = app.world.find_by_id(id).unwrap();
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.x >= Fixed::ZERO, "{id} starts before the viewport");
            assert!(rect.y >= Fixed::ZERO, "{id} starts above the viewport");
            assert!(
                rect.x + rect.w <= Fixed::from_int(width as i32),
                "{id} exceeds the viewport width at {width}x{height}",
            );
            assert!(
                rect.y + rect.h <= Fixed::from_int(height as i32),
                "{id} exceeds the viewport height at {width}x{height}",
            );
        }
    }
}

#[test]
fn setup_keeps_the_callers_active_theme() {
    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::light());
    let root = app.spawn_root().id();
    setup_app(&mut app, root);

    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Surface),
        Theme::light().resolve(ColorToken::Surface),
    );
}

#[test]
fn blur_motion_updates_only_the_visual_transform() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets);
    app.set_root(root);
    update_layout(&mut app.world, root, &Viewport::new(480, 320, Fixed::ONE));

    let overlay = app.world.find_by_id("effect_blur_overlay").unwrap();
    let left_before = app.world.get::<Style>(overlay).unwrap().layout.left;
    app.world.insert_resource(DeltaTimeMs(1_100));
    (BlurPan::system().run)(&mut app.world);

    assert!(
        app.world.get::<WidgetTransform>(overlay).unwrap().0.tx > Fixed::ZERO,
        "blur overlay must travel inside its live stage",
    );
    assert_eq!(
        app.world.get::<Style>(overlay).unwrap().layout.left,
        left_before,
        "visual motion must not dirty layout geometry",
    );
}
