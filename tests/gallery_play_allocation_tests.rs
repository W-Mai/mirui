#![cfg(feature = "gallery")]

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::core::reactive::flush_signal_dirty;
use mirui::ecs::{Entity, World};
use mirui::input::event::gesture::{GestureEvent, GestureSystem};
use mirui::input::event::hit_test::hit_test;
use mirui::input::event::input::InputEvent;
use mirui::input::event::{bubble_dispatch_at, dispatch_input};
use mirui::surface::FramebufferAccess;
use mirui::types::Fixed;
use mirui::ui::branch::is_effectively_hidden;
use mirui::ui::widgets::{Button, Slider, Text};
use mirui::ui::{ComputedRect, Parent};
use tracking_allocator::tracked_allocations;

const MAX_WARMED_RENDER_ALLOCATIONS: usize = 40;

fn is_hidden_in_tree(world: &World, mut entity: Entity) -> bool {
    loop {
        if is_effectively_hidden(world, entity) {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.0;
    }
}

fn try_tap_at(
    world: &mut World,
    root: Entity,
    x: Fixed,
    y: Fixed,
    now_ms: u32,
    width: u16,
    height: u16,
) -> Option<Entity> {
    dispatch_input(
        world,
        root,
        &InputEvent::PointerDown { id: 0, x, y },
        now_ms,
        width,
        height,
    );
    dispatch_input(
        world,
        root,
        &InputEvent::PointerUp { id: 0, x, y },
        now_ms + 50,
        width,
        height,
    );
    let event = {
        let gestures = &mut world.resource_mut::<GestureSystem>().unwrap().events;
        assert!(gestures.buffer.len() <= 1);
        gestures.buffer.pop()
    };
    let event = event?;
    let target = match &event {
        GestureEvent::Tap { target, .. } => *target,
        _ => panic!("pointer sequence did not produce a tap: {event:?}"),
    };
    bubble_dispatch_at(world, &event, now_ms + 50);
    Some(target)
}

fn tap_at(
    world: &mut World,
    root: Entity,
    x: Fixed,
    y: Fixed,
    now_ms: u32,
    width: u16,
    height: u16,
) -> Entity {
    try_tap_at(world, root, x, y, now_ms, width, height)
        .expect("pointer sequence did not reach a target")
}

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
    let branches = [
        app.world.find_by_id("marble_play_board").unwrap(),
        app.world.find_by_id("marble_scene_board").unwrap(),
        app.world.find_by_id("marble_settings_panel").unwrap(),
    ];
    let steps = [
        (
            "marble_nav_edit",
            [true, false, false],
            [true, false, false],
        ),
        (
            "marble_nav_scenes",
            [false, true, false],
            [false, true, false],
        ),
        (
            "marble_nav_settings",
            [false, false, true],
            [false, false, true],
        ),
        (
            "marble_nav_play",
            [false, false, false],
            [true, false, false],
        ),
    ];

    for (index, (id, visible, visible_branches)) in steps.into_iter().enumerate() {
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
                !is_hidden_in_tree(&app.world, entity),
                expected_visible,
                "{id} left an unexpected page visible"
            );
        }
        for (index, (branch_id, branch)) in [
            "marble_play_board",
            "marble_scene_board",
            "marble_settings_panel",
        ]
        .into_iter()
        .zip(branches)
        .enumerate()
        {
            assert_eq!(app.world.find_by_id(branch_id), Some(branch));
            assert_eq!(
                !is_hidden_in_tree(&app.world, branch),
                visible_branches[index],
                "{id} left {branch_id} in the wrong visibility state"
            );
        }
    }
}

#[test]
fn marble_scene_cards_ignore_gaps_and_margins_but_load_a_selected_scene() {
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

    let scenes_nav = app.world.find_by_id("marble_nav_scenes").unwrap();
    let nav_rect = app.world.get::<ComputedRect>(scenes_nav).unwrap().0;
    let x = nav_rect.x + nav_rect.w / Fixed::from_int(2);
    let y = nav_rect.y + nav_rect.h / Fixed::from_int(2);
    assert_eq!(
        tap_at(&mut app.world, root, x, y, 0, width, height),
        scenes_nav
    );
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();

    let scene_board = app.world.find_by_id("marble_scene_board").unwrap();
    let second_card = app.world.find_by_id("marble_scene_card_1").unwrap();
    let scene_label = app.world.find_by_id("marble_scene_1_name").unwrap();
    let status = app.world.find_by_id("marble_status").unwrap();
    assert!(!is_hidden_in_tree(&app.world, scene_board));
    assert!(!is_hidden_in_tree(&app.world, scene_label));
    assert_eq!(
        app.world.get::<Text>(status).unwrap().resolve(&app.world),
        "SCENES · CHOOSE A LITTLE WORLD"
    );

    let board_rect = app.world.get::<ComputedRect>(scene_board).unwrap().0;
    let label_rect = app.world.get::<ComputedRect>(scene_label).unwrap().0;
    assert_eq!(label_rect.x, board_rect.x + Fixed::from_int(183));
    assert_eq!(label_rect.y, board_rect.y + Fixed::from_int(118));
    let board_point = |local_x: i32, local_y: i32| {
        (
            board_rect.x + board_rect.w * Fixed::from_int(local_x) / Fixed::from_int(480),
            board_rect.y + board_rect.h * Fixed::from_int(local_y) / Fixed::from_int(199),
        )
    };
    for (index, local_x) in [12, 163, 314, 465].into_iter().enumerate() {
        let (x, y) = board_point(local_x, 60);
        assert_eq!(hit_test(&app.world, root, x, y, width, height), None);
        assert_eq!(
            try_tap_at(
                &mut app.world,
                root,
                x,
                y,
                1_000 * (index as u32 + 1),
                width,
                height,
            ),
            None
        );
        flush_signal_dirty(&mut app.world);
        app.render_dirty().unwrap();
        assert!(!is_hidden_in_tree(&app.world, scene_board), "x={local_x}");
        assert!(!is_hidden_in_tree(&app.world, scene_label), "x={local_x}");
        assert_eq!(
            app.world.get::<Text>(status).unwrap().resolve(&app.world),
            "SCENES · CHOOSE A LITTLE WORLD",
            "x={local_x} loaded a scene from outside its card"
        );
    }

    for (local_x, card_id) in [(40, "marble_scene_card_0"), (340, "marble_scene_card_2")] {
        let (x, y) = board_point(local_x, 126);
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            app.world.find_by_id(card_id),
            "{card_id} must own its painted area"
        );
    }
    for local_y in [17, 182] {
        let (x, y) = board_point(204, local_y);
        assert_eq!(
            hit_test(&app.world, root, x, y, width, height),
            None,
            "scene card must not claim y={local_y} outside its paint"
        );
    }

    let (x, y) = board_point(204, 126);
    let hit = hit_test(&app.world, root, x, y, width, height).unwrap();
    assert_eq!(
        hit, second_card,
        "scene label must use its card's hit target"
    );
    assert_eq!(
        tap_at(&mut app.world, root, x, y, 5_000, width, height),
        hit
    );
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();
    assert!(is_hidden_in_tree(&app.world, scene_board));
    assert_eq!(
        app.world.get::<Text>(status).unwrap().resolve(&app.world),
        "LIVE · DRAG EMPTY SPACE TO TILT"
    );

    let settings_nav = app.world.find_by_id("marble_nav_settings").unwrap();
    let nav_rect = app.world.get::<ComputedRect>(settings_nav).unwrap().0;
    let x = nav_rect.x + nav_rect.w / Fixed::from_int(2);
    let y = nav_rect.y + nav_rect.h / Fixed::from_int(2);
    assert_eq!(
        tap_at(&mut app.world, root, x, y, 6_000, width, height),
        settings_nav
    );
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();
    let bpm = app.world.find_by_id("marble_setting_bpm").unwrap();
    assert_eq!(
        app.world.get::<Text>(bpm).unwrap().resolve(&app.world),
        "TEMPO · 72 BPM"
    );
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
    assert!(is_hidden_in_tree(&app.world, inspector));

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
        assert_eq!(is_hidden_in_tree(&app.world, inspector), !open);
        assert!(!is_hidden_in_tree(&app.world, properties));

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
    assert!(!is_hidden_in_tree(&app.world, label));
    assert!(!is_hidden_in_tree(&app.world, slider));
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
        assert!(!is_hidden_in_tree(&app.world, label));
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
    #[cfg(feature = "audio")]
    {
        let audio = app.audio().unwrap();
        assert!(audio.set_muted(false));
        flush_signal_dirty(&mut app.world);
        app.render_dirty().unwrap();
        assert!(!audio.state().unwrap().muted);
        assert_eq!(tone_count.get(), 0);
    }

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
    assert!(!is_hidden_in_tree(&app.world, inspector));
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
