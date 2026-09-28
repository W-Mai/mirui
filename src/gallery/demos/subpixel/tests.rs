use super::composition::build_widgets;
use super::motion::bar_move_system;
use super::runtime::setup_app;
use super::state::{BAR_MARGIN, BAR_W, BarState, START_Y};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::render_system::update_layout;
use crate::ui::{Children, ComputedRect, IdMap, Style, UiScope};

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
fn stage_and_arena_fit_phone_and_desktop_viewports() {
    for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        for id in ["subpixel_stage", "subpixel_arena"] {
            let entity = app.world.find_by_id(id).unwrap();
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.x >= Fixed::ZERO);
            assert!(rect.y >= Fixed::ZERO);
            assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
            assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
        }
    }
}

#[test]
fn bars_follow_theme_and_live_arena_bounds() {
    let mut app = App::headless(320, 568);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    update_layout(&mut app.world, root, &Viewport::new(320, 568, Fixed::ONE));

    let snapped = app.world.find_by_id("subpixel_snapped_bar").unwrap();
    let smooth = app.world.find_by_id("subpixel_smooth_bar").unwrap();
    assert_eq!(
        app.world.get::<Style>(snapped).unwrap().bg_color,
        Some(ColorToken::Error.into()),
    );
    assert_eq!(
        app.world.get::<Style>(smooth).unwrap().bg_color,
        Some(ColorToken::Secondary.into()),
    );

    app.world.insert_resource(crate::ecs::DeltaTimeMs(250));
    bar_move_system(&mut app.world);
    let arena_width = app
        .world
        .get::<ComputedRect>(app.world.find_by_id("subpixel_arena").unwrap())
        .unwrap()
        .0
        .w
        .to_int();
    let smooth_state = app.world.get::<BarState>(smooth).unwrap();
    assert_eq!(
        smooth_state.x,
        Fixed::from_int(arena_width - BAR_W - BAR_MARGIN),
    );
    assert!(smooth_state.y > Fixed::from_int(START_Y));
}
