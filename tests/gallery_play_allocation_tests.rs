#![cfg(feature = "gallery")]

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::gesture::{GestureEvent, GestureSystem};
use mirui::input::event::hit_test::hit_test;
use mirui::input::event::input::InputEvent;
use mirui::input::event::{bubble_dispatch_at, dispatch_input};
use mirui::surface::FramebufferAccess;
use mirui::types::Fixed;
use mirui::ui::ComputedRect;
use mirui::ui::branch::is_effectively_hidden;
use mirui::ui::widgets::{Button, Slider, Text};
use tracking_allocator::tracked_allocations;

const MAX_WARMED_RENDER_ALLOCATIONS: usize = 40;

#[cfg(feature = "audio")]
struct CountingAudioSink(std::rc::Rc<std::cell::Cell<usize>>);

#[cfg(feature = "audio")]
impl mirui::audio::AudioSink for CountingAudioSink {
    type Error = std::convert::Infallible;

    fn start(&mut self, _: &'static mirui::audio::AudioBank) -> Result<(), Self::Error> {
        Ok(())
    }

    fn submit(&mut self, command: mirui::audio::AudioCommand) -> Result<(), Self::Error> {
        if matches!(command, mirui::audio::AudioCommand::Tone(_)) {
            self.0.set(self.0.get() + 1);
        }
        Ok(())
    }
}

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

#[test]
fn first_marble_inspector_open_reuses_input_layout_and_render_storage() {
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

    let edit = app.world.find_by_id("marble_nav_edit").unwrap();
    let properties = app.world.find_by_id("marble_properties").unwrap();
    let inspector = app.world.find_by_id("marble_inspector").unwrap();
    let close = app.world.find_by_id("marble_inspector_close").unwrap();
    assert!(is_effectively_hidden(&app.world, inspector));

    let frame_hash = |pixels: &[u8]| {
        pixels.iter().fold(0u64, |hash, byte| {
            hash.wrapping_mul(16_777_619) ^ u64::from(*byte)
        })
    };
    let mut previous_pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
    let mut allocation_counts = [(0, 0, 0); 4];
    let mut close_point = None;

    for (index, (target, open)) in [
        (edit, false),
        (properties, true),
        (close, false),
        (properties, true),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = app.world.get::<ComputedRect>(target).unwrap().0;
        let half = Fixed::from_ratio(1, 2);
        let x = rect.x + rect.w * half;
        let y = rect.y + rect.h * half;
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            Some(target)
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
            assert!(matches!(event, GestureEvent::Tap { target: tapped, .. } if tapped == target));
            bubble_dispatch_at(&mut app.world, &event, now_ms + 50);
        });
        let flush_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
        let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
        allocation_counts[index] = (tap_allocations, flush_allocations, render_allocations);

        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(app.last_text_content_failure(), None);
        assert_eq!(is_effectively_hidden(&app.world, inspector), !open);
        assert!(!is_effectively_hidden(&app.world, properties));

        let pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
        assert_ne!(
            pixels, previous_pixels,
            "step {index} did not change pixels"
        );
        previous_pixels = pixels;

        let properties_rect = app.world.get::<ComputedRect>(properties).unwrap().0;
        let px = properties_rect.x + properties_rect.w * half;
        let py = properties_rect.y + properties_rect.h * half;
        let properties_hit = hit_test(&app.world, root, px, py, width, height);
        if open {
            assert_ne!(properties_hit, Some(properties));
            let close_rect = app.world.get::<ComputedRect>(close).unwrap().0;
            let cx = close_rect.x + close_rect.w * half;
            let cy = close_rect.y + close_rect.h * half;
            close_point = Some((cx, cy));
            assert_eq!(
                hit_test(&app.world, root, cx, cy, width, height),
                Some(close)
            );
        } else {
            assert_eq!(properties_hit, Some(properties));
            if let Some((cx, cy)) = close_point {
                assert_ne!(
                    hit_test(&app.world, root, cx, cy, width, height),
                    Some(close)
                );
            }
        }
    }

    assert_eq!(
        allocation_counts,
        [(0, 0, 0); 4],
        "edit/open/close/reopen action, flush, and render allocations"
    );
}

#[test]
fn first_marble_bpm_updates_fit_bounded_text_without_allocations() {
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

    let settings = app.world.find_by_id("marble_nav_settings").unwrap();
    let settings_rect = app.world.get::<ComputedRect>(settings).unwrap().0;
    bubble_dispatch_at(
        &mut app.world,
        &GestureEvent::Tap {
            x: settings_rect.x + settings_rect.w / Fixed::from_int(2),
            y: settings_rect.y + settings_rect.h / Fixed::from_int(2),
            target: settings,
        },
        100,
    );
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();

    let label = app.world.find_by_id("marble_setting_bpm").unwrap();
    let slider = app.world.find_by_id("marble_setting_bpm_control").unwrap();
    assert!(!is_effectively_hidden(&app.world, label));
    assert!(!is_effectively_hidden(&app.world, slider));
    assert_eq!(
        app.world.get::<Text>(label).unwrap().resolve(&app.world),
        "TEMPO · 96 BPM"
    );
    assert_eq!(
        app.world.get::<Text>(label).unwrap().text_capacity(),
        Some("TEMPO · 160 BPM".len())
    );

    let slider_rect = app.world.get::<ComputedRect>(slider).unwrap().0;
    let y = slider_rect.y + slider_rect.h / Fixed::from_int(2);
    let frame_hash = |pixels: &[u8]| {
        pixels.iter().fold(0u64, |hash, byte| {
            hash.wrapping_mul(16_777_619) ^ u64::from(*byte)
        })
    };
    let mut previous_pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
    let mut allocation_counts = [(0, 0, 0); 2];

    for (index, (value, x, expected)) in [
        (
            160,
            slider_rect.x + slider_rect.w - Fixed::ONE,
            "TEMPO · 160 BPM",
        ),
        (55, slider_rect.x + Fixed::ONE, "TEMPO · 55 BPM"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            Some(slider)
        );
        let model_call_allocations = tracked_allocations(|| {
            bubble_dispatch_at(
                &mut app.world,
                &GestureEvent::Tap {
                    x,
                    y,
                    target: slider,
                },
                200 + value as u32,
            );
        });
        let flush_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
        let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());

        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(app.last_text_content_failure(), None);
        assert_eq!(
            app.world.get::<Text>(label).unwrap().resolve(&app.world),
            expected
        );
        assert_eq!(
            app.world.get::<Slider>(slider).unwrap().value,
            Fixed::from_int(value)
        );
        assert!(!is_effectively_hidden(&app.world, label));
        let pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
        assert_ne!(pixels, previous_pixels, "BPM {value} did not change pixels");
        previous_pixels = pixels;
        allocation_counts[index] = (
            model_call_allocations,
            flush_allocations,
            render_allocations,
        );
    }
    assert_eq!(allocation_counts, [(0, 0, 0); 2]);
}

#[test]
fn first_marble_inspector_pitch_and_timbre_actions_do_not_allocate() {
    let (width, height) = mirui::gallery::demos::marble_play::VIEWPORT;
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    app.with_text_layout_limits(mirui::text::TextLayoutLimits::EMBEDDED);
    let root = app.spawn_root().id();
    #[cfg(feature = "audio")]
    let tone_count = {
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        app.add_plugin(mirui::app::plugins::AudioPlugin::new(
            CountingAudioSink(count.clone()),
            mirui::gallery::demos::marble_play::audio_bank(),
        ));
        count
    };
    mirui::gallery::demos::marble_play::setup_app(&mut app, root);
    app.set_root(root);
    app.prepare_text_layout().unwrap();
    app.systems.run_all(&mut app.world);
    app.render().unwrap();

    for (index, id) in ["marble_nav_edit", "marble_properties"]
        .into_iter()
        .enumerate()
    {
        let target = app.world.find_by_id(id).unwrap();
        let rect = app.world.get::<ComputedRect>(target).unwrap().0;
        bubble_dispatch_at(
            &mut app.world,
            &GestureEvent::Tap {
                x: rect.x + rect.w / Fixed::from_int(2),
                y: rect.y + rect.h / Fixed::from_int(2),
                target,
            },
            index as u32 * 100 + 100,
        );
        flush_signal_dirty(&mut app.world);
        app.render_dirty().unwrap();
    }

    let inspector = app.world.find_by_id("marble_inspector").unwrap();
    let pitch = app.world.find_by_id("marble_pitch").unwrap();
    let pitch_up = app.world.find_by_id("marble_pitch_up").unwrap();
    let timbre = app.world.find_by_id("marble_timbre").unwrap();
    assert!(!is_effectively_hidden(&app.world, inspector));
    assert_eq!(
        app.world.get::<Text>(pitch).unwrap().resolve(&app.world),
        "C5"
    );
    assert!(app.world.has::<Button>(pitch_up));
    assert_eq!(
        app.world.get::<Text>(pitch_up).unwrap().resolve(&app.world),
        "+"
    );

    let frame_hash = |pixels: &[u8]| {
        pixels.iter().fold(0u64, |hash, byte| {
            hash.wrapping_mul(16_777_619) ^ u64::from(*byte)
        })
    };
    let mut previous_pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
    let mut allocation_counts = [(0, 0, 0); 2];
    for (index, (target, expected_pitch, expected_timbre)) in
        [(pitch_up, "D5", "MALLET"), (timbre, "D5", "SYNTH")]
            .into_iter()
            .enumerate()
    {
        let rect = app.world.get::<ComputedRect>(target).unwrap().0;
        let x = rect.x + rect.w / Fixed::from_int(2);
        let y = rect.y + rect.h / Fixed::from_int(2);
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            Some(target)
        );
        let now_ms = index as u32 * 1_000 + 300;
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
            assert!(matches!(event, GestureEvent::Tap { target: tapped, .. } if tapped == target));
            bubble_dispatch_at(&mut app.world, &event, now_ms + 50);
        });
        let flush_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
        let mut render_result = Ok(());
        let render_allocations = tracked_allocations(|| render_result = app.render_dirty());
        assert!(
            render_result.is_ok(),
            "render failed: {render_result:?}; text layout: {:?}; text content: {:?}",
            app.last_text_layout_failure(),
            app.last_text_content_failure()
        );
        allocation_counts[index] = (tap_allocations, flush_allocations, render_allocations);

        assert_eq!(app.last_text_layout_failure(), None);
        assert_eq!(app.last_text_content_failure(), None);
        assert_eq!(
            app.world.get::<Text>(pitch).unwrap().resolve(&app.world),
            expected_pitch
        );
        assert_eq!(
            app.world.get::<Text>(timbre).unwrap().resolve(&app.world),
            expected_timbre
        );
        #[cfg(feature = "audio")]
        assert_eq!(tone_count.get(), (index + 1) * 2);
        let pixels = frame_hash(app.backend.framebuffer().buf.as_slice());
        assert_ne!(
            pixels, previous_pixels,
            "Inspector action {index} did not repaint"
        );
        previous_pixels = pixels;
    }
    assert_eq!(
        allocation_counts,
        [(0, 0, 0); 2],
        "pitch/timbre action, flush, and render allocations"
    );
}
