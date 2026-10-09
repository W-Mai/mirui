use super::composition::build_widgets;
use super::state::{ACCENT, custom_theme, dark_with_accent, light_with_accent};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(480, 320);
    app.with_event_capacity(EventCapacity::new().commands(CommandCapacity::new().set_theme(1)));
    app.set_theme(dark_with_accent()).unwrap();
    app.register_theme(light_with_accent());
    app.register_theme(custom_theme());
    let parent = app.spawn_root().layout(LayoutStyle::default()).id();
    app.compose(parent, build_widgets());
    assert!(
        app.world
            .get::<crate::ui::Children>(parent)
            .is_some_and(|c| !c.0.is_empty())
    );
}

#[test]
fn tap_button_swaps_global_theme() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    super::setup_app(&mut app, root);
    let world = &mut app.world;
    let custom_btn = world.find_by_id("theme_custom").unwrap();

    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: custom_btn,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    let theme = world.resource::<Theme>().unwrap();
    assert_eq!(theme.resolve(ACCENT), Color::rgb(140, 200, 220));
}

#[test]
fn token_grid_stays_inside_landscape_and_portrait_viewports() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(480, 320), (320, 480)] {
        let mut app = App::headless(width, height);
        app.with_event_capacity(EventCapacity::new().commands(CommandCapacity::new().set_theme(1)));
        app.with_default_widgets()
            .with_default_systems()
            .with_theme(dark_with_accent());
        let root = app.spawn_root().id();
        app.compose(root, build_widgets());
        app.set_root(root);
        update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        let grid = app.world.find_by_id("theme_token_grid").unwrap();
        let accent = app.world.find_by_id("theme_accent_row").unwrap();
        let grid = app.world.get::<ComputedRect>(grid).unwrap().0;
        let accent = app.world.get::<ComputedRect>(accent).unwrap().0;

        assert!(grid.x >= Fixed::ZERO);
        assert!(grid.y >= Fixed::ZERO);
        assert!(grid.x + grid.w <= Fixed::from_int(width as i32));
        assert!(grid.y + grid.h <= Fixed::from_int(height as i32));
        assert!(accent.x >= grid.x);
        assert!(accent.y >= grid.y);
        assert!(accent.x + accent.w <= grid.x + grid.w);
        assert!(accent.y + accent.h <= grid.y + grid.h);
    }
}
