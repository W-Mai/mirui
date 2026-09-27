#![cfg(feature = "gallery")]

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::gesture::{GestureEvent, GestureSystem};
use mirui::input::event::hit_test::hit_test;
use mirui::input::event::input::InputEvent;
use mirui::input::event::{bubble_dispatch_at, dispatch_input};
use mirui::types::Fixed;
use mirui::ui::ComputedRect;
use mirui::ui::branch::is_effectively_hidden;
use tracking_allocator::tracked_allocations;

const MAX_WARMED_RENDER_ALLOCATIONS: usize = 40;

#[test]
fn warmed_marble_frame_stays_inside_allocation_budget() {
    let (width, height) = mirui::gallery::demos::marble_play::VIEWPORT;
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    mirui::gallery::demos::marble_play::setup_app(&mut app, root);
    app.set_root(root);

    for _ in 0..3 {
        app.systems.run_all(&mut app.world);
        app.render_dirty().unwrap();
    }
    let system_allocations = tracked_allocations(|| app.systems.run_all(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());

    let mut blank = App::headless(width, height);
    blank.with_default_widgets().with_default_systems();
    let blank_root = blank.spawn_root().id();
    blank.set_root(blank_root);
    for _ in 0..3 {
        blank.systems.run_all(&mut blank.world);
        blank.render_dirty().unwrap();
    }
    let blank_systems = tracked_allocations(|| blank.systems.run_all(&mut blank.world));
    let blank_render = tracked_allocations(|| blank.render_dirty().unwrap());

    assert!(blank_systems <= 1);
    assert_eq!(blank_render, 0);
    assert!(system_allocations <= 2);
    assert!(
        render_allocations <= MAX_WARMED_RENDER_ALLOCATIONS,
        "warmed render allocated {render_allocations} times"
    );
}

#[test]
fn first_marble_page_switch_accounts_for_tap_flush_and_render_allocations() {
    let (width, height) = mirui::gallery::demos::marble_play::VIEWPORT;
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    app.with_text_layout_limits(mirui::text::TextLayoutLimits::EMBEDDED);
    let root = app.spawn_root().id();
    mirui::gallery::demos::marble_play::setup_app(&mut app, root);
    app.set_root(root);
    app.prepare_text_layout().unwrap();
    app.systems.run_all(&mut app.world);
    app.render().unwrap();

    let edit = app.world.find_by_id("marble_properties").unwrap();
    let scenes = app.world.find_by_id("marble_scene_0_name").unwrap();
    let settings = app
        .world
        .find_by_id("marble_setting_gravity_control")
        .unwrap();
    let steps = [
        ("marble_nav_edit", [true, false, false]),
        ("marble_nav_scenes", [false, true, false]),
        ("marble_nav_settings", [false, false, true]),
        ("marble_nav_play", [false, false, false]),
    ];

    for (index, (id, visible)) in steps.into_iter().enumerate() {
        let button = app.world.find_by_id(id).unwrap();
        let rect = app.world.get::<ComputedRect>(button).unwrap().0;
        let half = Fixed::from_ratio(1, 2);
        let x = rect.x + rect.w * half;
        let y = rect.y + rect.h * half;
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            Some(button)
        );

        let now_ms = index as u32 * 1_000;
        let tap_allocations = tracked_allocations(|| {
            dispatch_input(
                &mut app.world,
                root,
                &InputEvent::PointerDown { id: 0, x, y },
                now_ms,
                width,
                height,
            );
            dispatch_input(
                &mut app.world,
                root,
                &InputEvent::PointerUp { id: 0, x, y },
                now_ms + 50,
                width,
                height,
            );
            let event = {
                let gestures = &mut app.world.resource_mut::<GestureSystem>().unwrap().events;
                assert_eq!(gestures.buffer.len(), 1);
                gestures.buffer.pop().unwrap()
            };
            assert!(matches!(event, GestureEvent::Tap { target, .. } if target == button));
            bubble_dispatch_at(&mut app.world, &event, now_ms + 50);
        });
        let flush_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
        let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
        assert_eq!(tap_allocations, 0, "{id} tap allocated");
        assert_eq!(flush_allocations, 0, "{id} signal flush allocated");
        assert_eq!(render_allocations, 0, "{id} first dirty render allocated");
        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(app.last_text_content_failure(), None);

        for (entity, expected_visible) in [
            (edit, visible[0]),
            (scenes, visible[1]),
            (settings, visible[2]),
        ] {
            assert_eq!(
                !is_effectively_hidden(&app.world, entity),
                expected_visible,
                "{id} left an unexpected page visible"
            );
        }
    }
}
