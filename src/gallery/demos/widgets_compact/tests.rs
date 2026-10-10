use super::VIEWPORT;
use super::binding::VIRTUAL_ITEM_COUNT;
use super::runtime::{automation, install};
use super::style::CompactTheme;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Viewport;
use crate::ui::widgets::{LazyList, ProgressBar, Slider, TabContent};
use crate::ui::{ComputedRect, Theme};

#[test]
fn compact_widgets_fit_and_render_at_native_size() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    flush_signal_dirty(&mut app.world);
    app.render().unwrap();

    let shell = app
        .world
        .find_by_id("compact_widgets_shell")
        .expect("compact shell");
    let shell_rect = app.world.get::<ComputedRect>(shell).expect("shell rect").0;
    let progress = app
        .world
        .find_by_id("compact_widgets_progress")
        .expect("compact progress");

    assert_eq!(shell_rect.w.to_int(), 128);
    assert_eq!(shell_rect.h.to_int(), 128);
    assert_eq!(
        app.world.get::<ProgressBar>(progress).unwrap().value,
        Fixed::from_ratio(62, 100).to_f32(),
    );
    assert!(
        app.backend
            .framebuffer()
            .buf
            .as_slice()
            .iter()
            .any(|byte| *byte != 0),
    );
}

#[test]
fn compact_progress_tracks_slider_events_without_polling() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    app.render().unwrap();

    let slider = app.world.find_by_id("compact_widgets_slider").unwrap();
    let progress = app.world.find_by_id("compact_widgets_progress").unwrap();
    let old = app.world.get::<Slider>(slider).unwrap().value;
    let new = Fixed::from_int(25);
    app.world.get_mut::<Slider>(slider).unwrap().value = new;
    {
        crate::input::event::runtime::emit_widget(
            &mut app.world,
            slider,
            crate::ui::widgets::slider::ValueChanged { new, old },
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
    };
    flush_signal_dirty(&mut app.world);

    assert_ne!(
        app.world.get::<Slider>(slider).unwrap().ratio(),
        Fixed::from_ratio(62, 100),
    );
    assert_eq!(
        app.world.get::<ProgressBar>(progress).unwrap().value,
        app.world.get::<Slider>(slider).unwrap().ratio().to_f32(),
    );
}

fn compact_layout(width: u16, height: u16) -> (crate::types::Rect, crate::types::Rect) {
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    app.render().unwrap();

    let header = app.world.find_by_id("compact_widgets_header").unwrap();
    let body = app.world.find_by_id("compact_widgets_body").unwrap();
    (
        app.world.get::<ComputedRect>(header).unwrap().0,
        app.world.get::<ComputedRect>(body).unwrap().0,
    )
}

#[test]
fn compact_widgets_reflow_between_portrait_and_landscape() {
    let (portrait_header, portrait_body) = compact_layout(96, 160);
    let (square_header, square_body) = compact_layout(128, 128);
    let (landscape_header, landscape_body) = compact_layout(160, 96);

    assert!(portrait_body.y >= portrait_header.y + portrait_header.h);
    assert!(square_body.y >= square_header.y + square_header.h);
    assert!(landscape_body.x >= landscape_header.x + landscape_header.w);
    assert!(landscape_body.y <= landscape_header.y + Fixed::from_int(1));
    assert!(portrait_body.w < square_body.w);
    assert!(landscape_body.w > portrait_body.w);
}

#[test]
fn compact_widgets_reflow_after_live_viewport_changes() {
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    app.render().unwrap();

    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &Viewport::new(160, 96, Fixed::ONE),
    );
    let shell = app.world.find_by_id("compact_widgets_shell").unwrap();
    assert_eq!(
        app.world.get::<Style>(shell).unwrap().layout.direction,
        FlexDirection::Row,
    );

    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &Viewport::new(96, 160, Fixed::ONE),
    );
    assert_eq!(
        app.world.get::<Style>(shell).unwrap().layout.direction,
        FlexDirection::Column,
    );
}

#[test]
fn compact_tabs_keep_distinct_pages_and_a_bounded_virtual_pool() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);
    let timeline = app.compose(root, automation()).unwrap();

    let mut pages: alloc::vec::Vec<u8> = app
        .world
        .query::<TabContent>()
        .iter()
        .map(|(_, page)| page.index)
        .collect();
    pages.sort_unstable();
    let list = app.world.find_by_id("compact_widgets_list").unwrap();
    assert!(
        app.world
            .get::<crate::ui::Children>(list)
            .unwrap()
            .0
            .is_empty()
    );
    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );
    let list_state = app.world.get::<LazyList>(list).unwrap();
    let cells = &app.world.get::<crate::ui::Children>(list).unwrap().0;
    let light = app.world.find_by_id("compact_theme_light").unwrap();
    let dark = app.world.find_by_id("compact_theme_dark").unwrap();
    let light_surface = app
        .world
        .get::<CompactTheme>(light)
        .unwrap()
        .0
        .resolve(ColorToken::Surface);
    let dark_surface = app
        .world
        .get::<CompactTheme>(dark)
        .unwrap()
        .0
        .resolve(ColorToken::Surface);

    assert_eq!(pages, [0, 1, 2]);
    assert_eq!(list_state.items.len(), VIRTUAL_ITEM_COUNT as usize);
    assert!(cells.len() < 32);
    assert_ne!(light_surface, dark_surface);
    assert_eq!(timeline.total_ms, 9_000);
}

#[test]
fn automation_requires_its_registered_targets() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    assert!(app.compose(root, automation()).is_none());

    install(&mut app, root);
    let timeline = app.compose(root, automation()).unwrap();
    assert_eq!(timeline.total_ms, 9_000);
    assert!(timeline.is_running());
    app.world
        .resource_mut::<crate::ui::IdMap>()
        .unwrap()
        .remove("compact_theme_dark");
    assert!(app.compose(root, automation()).is_none());
}

#[test]
fn compact_theme_buttons_replace_the_active_theme() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark());
    let root = app.spawn_root().id();
    install(&mut app, root);
    app.set_root(root);

    let light = app.world.find_by_id("compact_theme_light").unwrap();
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            &mut app.world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: light,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Surface),
        Theme::light().resolve(ColorToken::Surface),
    );

    let dark = app.world.find_by_id("compact_theme_dark").unwrap();
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            &mut app.world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: dark,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Surface),
        Theme::dark().resolve(ColorToken::Surface),
    );
}
