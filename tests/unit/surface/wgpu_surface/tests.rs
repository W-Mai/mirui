use super::*;
use crate::{app::App, ecs::Entity, input::event::focus::FocusState, ui::widgets::TextInput};

#[test]
fn platform_redraw_and_restore_remain_pending_until_acceptance() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    for event in [WindowEvent::RedrawRequested, WindowEvent::Occluded(false)] {
        runtime.needs_redraw = false;
        assert!(!runtime.window_event(event));
        assert!(runtime.needs_redraw);
        runtime.finish_input_batch();
        assert!(runtime.needs_redraw);
        assert_eq!(
            runtime.begin_frame(),
            Err(crate::render::RenderError::SurfaceUnavailable)
        );
        assert!(runtime.needs_redraw);
    }
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn native_fixture() -> (TestApp, Entity, WgpuRuntime) {
    #[crate::compose]
    fn editor() -> Entity {
        crate::ui! {
            TextInput (width: 160, height: 36)
        }
    }
    let mut app = App::headless(200, 100);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let editor = app.compose(root, editor());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
    (app, editor, runtime)
}

fn deliver(app: &mut TestApp, runtime: &mut WgpuRuntime) {
    while let Some(event) = runtime.poll_event() {
        crate::input::event::deliver_input(&mut app.world, None, &event, 0, 200, 100);
    }
}

fn display(app: &TestApp, editor: Entity, expected: &str) {
    app.world
        .get::<crate::ui::widgets::TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, expected));
}

#[test]
fn host_state_preserves_input_order_without_requesting_suspend() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    runtime.pending_move = Some((Fixed::ONE, Fixed::ZERO));
    for event in [
        WindowEvent::Focused(false),
        WindowEvent::Occluded(true),
        WindowEvent::Focused(true),
        WindowEvent::Occluded(false),
    ] {
        assert!(!runtime.window_event(event));
    }
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerMove { .. })
    ));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostFocus { focused: false })
    ));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostVisibility { visible: false })
    ));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostFocus { focused: true })
    ));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostVisibility { visible: true })
    ));
    assert!(runtime.poll_event().is_none());
}

#[test]
fn pointer_moves_only_coalesce_between_ordered_input_boundaries() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    let device_id = winit::event::DeviceId::dummy();
    let moved = |x| WindowEvent::CursorMoved {
        device_id,
        position: winit::dpi::PhysicalPosition::new(x, 0.0),
    };
    let button = |state| WindowEvent::MouseInput {
        device_id,
        state,
        button: MouseButton::Left,
    };

    runtime.window_event(moved(1.0));
    runtime.window_event(moved(2.0));
    runtime.window_event(button(ElementState::Pressed));
    runtime.window_event(moved(3.0));
    runtime.window_event(moved(4.0));
    runtime.window_event(button(ElementState::Released));
    runtime.window_event(moved(5.0));
    runtime.window_event(WindowEvent::Focused(true));

    assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(2)));
    assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerDown { x, .. }) if x == Fixed::from_int(2)));
    assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(4)));
    assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerUp { x, .. }) if x == Fixed::from_int(4)));
    assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(5)));
    assert!(matches!(
        runtime.event_queue.pop_front(),
        Some(InputEvent::HostFocus { focused: true })
    ));
    assert!(runtime.event_queue.is_empty());
    assert!(runtime.pending_move.is_none());
}

#[test]
fn polling_retains_trailing_moves_and_reuses_queue_storage() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    runtime.event_queue.reserve(2);
    let allocations = crate::test_support::tracked_allocations(|| {
        for x in 0..8 {
            runtime.event_queue.push_back(InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(x),
                y: Fixed::ZERO,
            });
            runtime.pending_move = Some((Fixed::from_int(x + 1), Fixed::ZERO));
            assert!(matches!(
                runtime.poll_event(),
                Some(InputEvent::PointerDown { .. })
            ));
            assert!(matches!(runtime.poll_event(),
                    Some(InputEvent::PointerMove { x: next, .. }) if next == Fixed::from_int(x + 1)));
            assert!(runtime.poll_event().is_none());
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn cursor_boundaries_preserve_position_and_native_input_order() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    let device_id = winit::event::DeviceId::dummy();
    runtime.event_queue.reserve(4);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            runtime.window_event(WindowEvent::CursorEntered { device_id });
            runtime.window_event(WindowEvent::CursorMoved {
                device_id,
                position: winit::dpi::PhysicalPosition::new(18.0, 24.0),
            });
            runtime.window_event(WindowEvent::CursorLeft { device_id });
            runtime.window_event(WindowEvent::MouseInput {
                device_id,
                state: ElementState::Released,
                button: MouseButton::Left,
            });
        }),
        0
    );
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerEnter {
            kind: crate::surface::PointerKind::Mouse,
            id: 0
        })
    ));
    assert!(
        matches!(runtime.poll_event(), Some(InputEvent::PointerMove { x, y, .. }) if x == Fixed::from_int(18) && y == Fixed::from_int(24))
    );
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerLeave {
            kind: crate::surface::PointerKind::Mouse,
            id: 0
        })
    ));
    assert!(
        matches!(runtime.poll_event(), Some(InputEvent::PointerUp { x, y, .. }) if x == Fixed::from_int(18) && y == Fixed::from_int(24))
    );
    assert!(runtime.poll_event().is_none());
    assert_eq!(
        runtime.last_cursor,
        (Fixed::from_int(18), Fixed::from_int(24))
    );
}

#[test]
fn keyboard_translation_preserves_pressed_and_released_states() {
    use crate::input::event::input::{
        KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
    };

    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    for (key, expected) in [
        (NamedKey::Backspace, KEY_BACKSPACE),
        (NamedKey::Delete, KEY_DELETE),
        (NamedKey::ArrowLeft, KEY_LEFT),
        (NamedKey::ArrowRight, KEY_RIGHT),
        (NamedKey::Home, KEY_HOME),
        (NamedKey::End, KEY_END),
        (NamedKey::Enter, KEY_RETURN),
        (NamedKey::Escape, crate::input::event::input::KEY_ESCAPE),
    ] {
        for state in [ElementState::Pressed, ElementState::Released] {
            assert!(!runtime.keyboard_input(&Key::Named(key), state, None, false, None));
            assert!(matches!(runtime.event_queue.pop_front(),
                    Some(InputEvent::Key { code, pressed, .. })
                    if code == expected && pressed == (state == ElementState::Pressed)));
            assert!(runtime.event_queue.is_empty());
        }
    }
}

#[test]
fn physical_position_and_committed_character_remain_independent() {
    use crate::input::event::input::{KEY_UNKNOWN, PhysicalKey};
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    for state in [ElementState::Pressed, ElementState::Released] {
        runtime.keyboard_input(
            &Key::Character("z".into()),
            state,
            Some("z"),
            true,
            Some(PhysicalKey::KeyW),
        );
        assert!(matches!(runtime.poll_event(), Some(InputEvent::Key {
                physical: Some(PhysicalKey::KeyW), code: KEY_UNKNOWN, repeat: Some(true), pressed, ..
            }) if pressed == (state == ElementState::Pressed)));
        if state == ElementState::Pressed {
            assert!(
                matches!(runtime.poll_event(), Some(InputEvent::TextCommitted(text)) if text.text() == "z")
            );
        }
        assert!(runtime.poll_event().is_none());
    }
}

#[test]
fn keyboard_translation_does_not_duplicate_quit_or_text() {
    let (mut app, editor, mut runtime) = native_fixture();
    {
        let key = NamedKey::BrowserBack;
        assert!(runtime.keyboard_input(&Key::Named(key), ElementState::Pressed, None, false, None));
        assert!(matches!(
            runtime.event_queue.pop_front(),
            Some(InputEvent::Quit)
        ));
        assert!(!runtime.keyboard_input(
            &Key::Named(key),
            ElementState::Released,
            None,
            false,
            None
        ));
        assert!(runtime.event_queue.is_empty());
    }
    let key = Key::Character("A中🙂".into());
    assert!(!runtime.keyboard_input(&key, ElementState::Pressed, Some("A中🙂"), false, None));
    assert_eq!(runtime.event_queue.len(), 1);
    deliver(&mut app, &mut runtime);
    display(&app, editor, "A中🙂");
    assert!(!runtime.keyboard_input(&key, ElementState::Released, Some("A中🙂"), false, None));
    assert!(runtime.event_queue.is_empty());
}

#[derive(Default)]
struct Received(alloc::string::String);

#[crate::event]
fn receive(
    input: event!(crate::input::event::input::TextCommitted),
    received: res!(mut Received),
) -> bool {
    received.0.push_str(input.text());
    true
}

#[test]
fn keyboard_text_reaches_app_callbacks_without_an_editor() {
    use crate::input::event::{contract::Event, input::TextCommitted};
    let mut app = App::headless(200, 100);
    app.add_resource(Received::default());
    app.on_event(TextCommitted::EVENT, receive::callback());
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    for text in ["w", " ", "A中🙂"] {
        runtime.keyboard_input(
            &Key::Character(text.into()),
            ElementState::Pressed,
            Some(text),
            false,
            None,
        );
        runtime.keyboard_input(
            &Key::Character(text.into()),
            ElementState::Released,
            Some(text),
            false,
            None,
        );
    }
    deliver(&mut app, &mut runtime);
    assert_eq!(app.world.resource::<Received>().unwrap().0, "w A中🙂");
}

#[test]
fn keyboard_metadata_is_captured_in_event_order_and_cleared_on_focus_loss() {
    use crate::input::event::input::KeyModifiers;
    use winit::keyboard::ModifiersState;
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    let key = Key::Named(NamedKey::ArrowLeft);
    runtime.keyboard_input(&key, ElementState::Pressed, None, false, None);
    runtime.window_event(WindowEvent::ModifiersChanged(
        (ModifiersState::SHIFT | ModifiersState::SUPER).into(),
    ));
    runtime.keyboard_input(&key, ElementState::Pressed, None, true, None);
    runtime.window_event(WindowEvent::ModifiersChanged(
        ModifiersState::empty().into(),
    ));
    runtime.keyboard_input(&key, ElementState::Released, None, false, None);
    for (expected, repeated) in [
        (None, false),
        (
            Some(KeyModifiers {
                shift: true,
                meta: true,
                ..Default::default()
            }),
            true,
        ),
        (Some(KeyModifiers::default()), false),
    ] {
        let Some(InputEvent::Key {
            modifiers, repeat, ..
        }) = runtime.poll_event()
        else {
            panic!("key")
        };
        assert_eq!(modifiers, expected);
        assert_eq!(repeat, Some(repeated));
    }
    runtime.window_event(WindowEvent::Focused(false));
    assert_eq!(runtime.modifiers, None);
    runtime.window_event(WindowEvent::ModifiersChanged(ModifiersState::ALT.into()));
    runtime.suspend(false);
    assert_eq!(runtime.modifiers, None);
}

#[test]
fn suspend_discards_transient_input_and_preserves_only_retained_window_state() {
    for retain_window in [false, true] {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        runtime.event_queue.push_back(InputEvent::PointerDown {
            kind: crate::input::event::input::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(4),
            y: Fixed::from_int(8),
        });
        runtime.pending_move = Some((Fixed::from_int(10), Fixed::from_int(12)));
        runtime
            .event_queue
            .push_back(InputEvent::HostFocus { focused: true });
        runtime.reconfigure_before_acquire = true;

        runtime.suspend(retain_window);

        if retain_window {
            assert!(matches!(
                runtime.poll_event(),
                Some(InputEvent::HostFocus { focused: true })
            ));
        }
        assert!(runtime.poll_event().is_none());
        assert!(runtime.pending_move.is_none());
        assert_eq!(runtime.reconfigure_before_acquire, retain_window);
    }
}

#[test]
fn suspend_retains_clipboard_receipts_until_the_app_consumes_them() {
    use crate::ui::clipboard::{self, ClipboardData, ClipboardRequest};
    for retain_window in [false, true] {
        let (mut app, editor, mut runtime) = native_fixture();
        clipboard::paste(&mut app.world, editor);
        let Some(ClipboardRequest::Read(request)) = clipboard::take_request(&mut app.world) else {
            panic!("expected pending clipboard read");
        };
        runtime.event_queue.push_back(InputEvent::Clipboard(
            request.complete(ClipboardData::text("retained")),
        ));
        runtime.suspend(retain_window);
        let Some(InputEvent::Clipboard(response)) = runtime.poll_event() else {
            panic!("lost clipboard receipt");
        };
        clipboard::complete(&mut app.world, &response);
        display(&app, editor, "retained");
        assert!(runtime.poll_event().is_none());
    }
}

#[test]
fn only_the_active_native_window_dispatches_events() {
    for event in [WindowEvent::RedrawRequested, WindowEvent::Focused(true)] {
        assert!(window_event_is_deliverable(Some(&7), &7, false, &event));
        assert!(!window_event_is_deliverable(Some(&7), &8, false, &event));
        assert!(!window_event_is_deliverable::<u8>(None, &7, false, &event));
    }
}

#[test]
fn suspended_windows_deliver_host_state_without_rendering_or_pointer_input() {
    let (mut app, _, mut runtime) = native_fixture();
    for event in [WindowEvent::Focused(false), WindowEvent::Occluded(true)] {
        runtime.window_event(event);
    }
    deliver(&mut app, &mut runtime);
    app.suspend();
    runtime.suspend(true);
    assert!(crate::ui::text_input::native_state(&mut app.world, 0).is_none());

    for event in [
        WindowEvent::Resized(winit::dpi::PhysicalSize::new(400, 200)),
        WindowEvent::RedrawRequested,
        WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        },
    ] {
        assert!(!window_event_is_deliverable(Some(&7), &7, true, &event));
    }
    for event in [WindowEvent::Focused(true), WindowEvent::Occluded(false)] {
        assert!(window_event_is_deliverable(Some(&7), &7, true, &event));
        runtime.window_event(event);
    }
    app.suspend();
    runtime.suspend(true);
    assert!(crate::ui::text_input::native_state(&mut app.world, 0).is_none());
    app.resume();
    deliver(&mut app, &mut runtime);
    assert!(crate::ui::text_input::native_state(&mut app.world, 0).is_some());
}

#[test]
fn physical_system_insets_are_reported_in_logical_pixels() {
    assert_eq!(
        safe_area_from_physical(1080, 2400, Fixed::from_int(3), 0, 72, 1080, 2256),
        SafeAreaInsets {
            top: Fixed::from_int(24),
            right: Fixed::ZERO,
            bottom: Fixed::from_int(48),
            left: Fixed::ZERO,
        }
    );
}

#[test]
fn opaque_touch_ids_map_to_stable_compact_slots() {
    let mut touches = NativeTouchMap::<2>::new();
    let x = Fixed::from_int(7);
    let y = Fixed::from_int(11);
    let native = 0x0000_7fff_1234_5678;
    assert!(matches!(
        touches.input_event(native, TouchPhase::Started, x, y),
        Some(InputEvent::PointerDown { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
    ));
    assert!(
        touches
            .input_event(native, TouchPhase::Started, x, y)
            .is_none()
    );
    assert!(matches!(
        touches.input_event(native, TouchPhase::Moved, x, y),
        Some(InputEvent::PointerMove { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
    ));
    assert!(matches!(
        touches.input_event(native, TouchPhase::Cancelled, x, y),
        Some(InputEvent::PointerCancel { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
    ));
    assert!(matches!(
        touches.input_event(native + 1, TouchPhase::Started, x, y),
        Some(InputEvent::PointerDown { id: 1, .. })
    ));
}

#[test]
fn native_touch_and_mouse_slots_stay_distinct_until_host_cancellation() {
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    let device_id = winit::event::DeviceId::dummy();
    let touch = |phase| {
        WindowEvent::Touch(winit::event::Touch {
            device_id,
            phase,
            location: winit::dpi::PhysicalPosition::new(7.0, 11.0),
            force: None,
            id: u64::MAX,
        })
    };
    runtime.window_event(touch(TouchPhase::Started));
    runtime.window_event(WindowEvent::MouseInput {
        device_id,
        state: ElementState::Pressed,
        button: MouseButton::Left,
    });
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerDown { id: 1, .. })
    ));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerDown { id: 0, .. })
    ));
    runtime.window_event(WindowEvent::Focused(false));
    runtime.window_event(touch(TouchPhase::Moved));
    runtime.window_event(touch(TouchPhase::Ended));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostFocus { focused: false })
    ));
    assert!(runtime.poll_event().is_none());
    runtime.window_event(touch(TouchPhase::Started));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::PointerDown { id: 1, .. })
    ));
    runtime.window_event(WindowEvent::Occluded(true));
    runtime.window_event(touch(TouchPhase::Cancelled));
    assert!(matches!(
        runtime.poll_event(),
        Some(InputEvent::HostVisibility { visible: false })
    ));
    assert!(runtime.poll_event().is_none());
}

#[test]
fn touch_map_preserves_established_contacts_when_full() {
    let mut touches = NativeTouchMap::<2>::new();
    let x = Fixed::ZERO;
    let y = Fixed::ZERO;
    assert!(touches.input_event(10, TouchPhase::Started, x, y).is_some());
    assert!(touches.input_event(20, TouchPhase::Started, x, y).is_some());
    assert!(touches.input_event(30, TouchPhase::Started, x, y).is_none());
    assert!(matches!(
        touches.input_event(20, TouchPhase::Moved, x, y),
        Some(InputEvent::PointerMove { id: 2, .. })
    ));
    touches.clear();
    assert!(touches.input_event(20, TouchPhase::Moved, x, y).is_none());
}

#[test]
fn fractional_device_scale_preserves_logical_coordinates() {
    assert_eq!(
        physical_to_logical(3.0, 6.0, 1.5),
        (Fixed::from_int(2), Fixed::from_int(4))
    );
    assert_eq!(
        physical_to_logical(12.0, 20.0, 0.0),
        (Fixed::from_int(12), Fixed::from_int(20))
    );
}

#[test]
fn wheel_units_preserve_pixels_and_normalize_lines_independently_of_dpi() {
    use winit::dpi::PhysicalPosition;
    assert_eq!(
        wheel_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.75, -0.375)),
            1.5
        ),
        (Fixed::from_ratio(-1, 2), Fixed::from_ratio(-1, 4))
    );
    assert_eq!(
        wheel_delta(MouseScrollDelta::LineDelta(0.125, -0.5), 2.0),
        (Fixed::from_ratio(-5, 2), Fixed::from_int(-10))
    );
}

#[test]
fn selection_shortcuts_do_not_duplicate_reported_keyboard_text() {
    use crate::input::event::input::{KEY_A, KEY_C, KEY_V, KEY_X, KeyModifiers};
    let mut runtime = WgpuRuntime::new(WgpuConfig::default());
    for modifiers in [
        KeyModifiers {
            control: true,
            ..Default::default()
        },
        KeyModifiers {
            meta: true,
            ..Default::default()
        },
    ] {
        runtime.modifiers = Some(modifiers);
        for (text, code) in [("a", KEY_A), ("C", KEY_C), ("v", KEY_V), ("X", KEY_X)] {
            for state in [ElementState::Pressed, ElementState::Released] {
                runtime.keyboard_input(&Key::Character(text.into()), state, Some(text), true, None);
                assert!(
                    matches!(runtime.event_queue.pop_front(), Some(InputEvent::Key { physical: None, code: actual, pressed, modifiers: Some(actual_modifiers), repeat: Some(true) }) if actual == code && pressed == (state == ElementState::Pressed) && actual_modifiers == modifiers)
                );
                assert!(runtime.event_queue.is_empty());
            }
        }
    }
}

#[test]
fn native_preedit_and_commit_share_a_session_without_duplicate_keyboard_text() {
    let (mut app, editor, mut runtime) = native_fixture();
    runtime.window_event(WindowEvent::Ime(Ime::Enabled));
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("ni".into(), Some((2, 2)))));
    deliver(&mut app, &mut runtime);
    display(&app, editor, "ni");
    assert!(!runtime.keyboard_input(
        &Key::Character("n".into()),
        ElementState::Pressed,
        Some("n"),
        false,
        None
    ));
    assert!(runtime.event_queue.is_empty());
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
    runtime.window_event(WindowEvent::Ime(Ime::Commit("你".into())));
    runtime.finish_input_batch();
    assert_eq!(runtime.event_queue.len(), 2);
    deliver(&mut app, &mut runtime);
    display(&app, editor, "你");
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("hao".into(), Some((3, 3)))));
    deliver(&mut app, &mut runtime);
    display(&app, editor, "你hao");
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
    runtime.finish_input_batch();
    assert_eq!(runtime.event_queue.len(), 2);
    deliver(&mut app, &mut runtime);
    display(&app, editor, "你");
    assert!(!runtime.text_input.composing());
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("x".into(), Some((1, 1)))));
    runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
    runtime.keyboard_input(
        &Key::Character("a".into()),
        ElementState::Pressed,
        Some("a"),
        false,
        None,
    );
    runtime.finish_input_batch();
    assert_eq!(runtime.event_queue.len(), 3);
    deliver(&mut app, &mut runtime);
    display(&app, editor, "你a");
}

#[test]
fn owned_commit_moves_into_the_prepared_queue_without_copying() {
    let (mut app, editor, mut runtime) = native_fixture();
    runtime.event_queue.reserve(1);
    runtime.window_event(WindowEvent::Ime(Ime::Enabled));
    let text = alloc::string::String::from("A中🙂");
    let allocations = crate::test_support::tracked_allocations(|| {
        runtime.window_event(WindowEvent::Ime(Ime::Commit(text)));
    });
    assert_eq!(allocations, 0);
    deliver(&mut app, &mut runtime);
    display(&app, editor, "A中🙂");
}

#[test]
fn retired_ime_source_is_not_retagged_when_the_editor_session_changes() {
    let (mut app, editor, mut runtime) = native_fixture();
    runtime.window_event(WindowEvent::Ime(Ime::Enabled));
    runtime.window_event(WindowEvent::Ime(Ime::Commit("old".into())));
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
    runtime.window_event(WindowEvent::Ime(Ime::Commit("late".into())));
    deliver(&mut app, &mut runtime);
    display(&app, editor, "");
    runtime.window_event(WindowEvent::Ime(Ime::Disabled));
    runtime.window_event(WindowEvent::Ime(Ime::Enabled));
    runtime.window_event(WindowEvent::Ime(Ime::Commit("new".into())));
    deliver(&mut app, &mut runtime);
    display(&app, editor, "new");
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
    app.add_resource(Received::default());
    app.on_event(
        <crate::input::event::input::TextCommitted as crate::input::event::contract::Event>::EVENT,
        receive::callback(),
    );
    runtime.window_event(WindowEvent::Ime(Ime::Commit("retired".into())));
    runtime.keyboard_input(
        &Key::Character("x".into()),
        ElementState::Pressed,
        Some("x"),
        false,
        None,
    );
    deliver(&mut app, &mut runtime);
    display(&app, editor, "new");
    assert_eq!(app.world.resource::<Received>().unwrap().0, "x");
}

#[test]
fn software_buffer_layout_applies_scale_and_budget() {
    assert_eq!(
        software_buffer_layout(
            Fixed::from_int(390),
            Fixed::from_int(844),
            Fixed::ONE,
            16 * 1024 * 1024,
        ),
        Ok((390, 844, 390 * 844 * 4))
    );
    assert_eq!(
        software_buffer_layout(
            Fixed::from_int(480),
            Fixed::from_int(1024),
            Fixed::from_int(3),
            16 * 1024 * 1024,
        ),
        Err(MobileSurfaceError::FramebufferBudget {
            required: 1440 * 3072 * 4,
            budget: 16 * 1024 * 1024,
        })
    );
}

#[test]
fn software_buffer_layout_rejects_non_positive_scale() {
    assert_eq!(
        software_buffer_layout(
            Fixed::from_int(400),
            Fixed::from_int(800),
            Fixed::ZERO,
            1024 * 1024,
        ),
        Err(MobileSurfaceError::InvalidRenderScale)
    );
}

#[test]
fn device_scale_uses_native_pixels_when_the_budget_allows() {
    assert_eq!(
        resolve_software_buffer_layout(
            Fixed::from_int(360),
            Fixed::from_int(800),
            Fixed::from_int(3),
            SoftwareRenderScale::Device,
            32 * 1024 * 1024,
        ),
        Ok((Fixed::from_int(3), 1080, 2400, 1080 * 2400 * 4))
    );
}

#[test]
fn device_scale_chooses_the_highest_q24_8_scale_inside_the_budget() {
    let budget = 360 * 800 * 4;
    let (scale, width, height, required) = resolve_software_buffer_layout(
        Fixed::from_int(360),
        Fixed::from_int(800),
        Fixed::from_int(3),
        SoftwareRenderScale::Device,
        budget,
    )
    .unwrap();
    assert_eq!(scale, Fixed::ONE);
    assert_eq!((width, height, required), (360, 800, budget));
}

#[test]
fn resume_rebuilds_a_dropped_presenter_at_the_same_size() {
    assert!(software_presenter_needs_rebuild(
        1080, 2400, 1080, 2400, false
    ));
    assert!(!software_presenter_needs_rebuild(
        1080, 2400, 1080, 2400, true
    ));
    assert!(software_presenter_needs_rebuild(
        1080, 2400, 1200, 2400, true
    ));
}

#[test]
fn software_upload_region_uses_full_stride_without_copying_rows() {
    let area = crate::types::PhysicalRect::new(7, 5, 20, 9).unwrap();
    assert_eq!(
        software_upload_region(64, 32, area),
        Some(SoftwareUploadRegion {
            offset: 5 * 64 * 4 + 7 * 4,
            bytes_per_row: 64 * 4,
            width: 20,
            height: 9,
        })
    );
    assert!(
        software_upload_region(
            64,
            32,
            crate::types::PhysicalRect::new(60, 0, 8, 1).unwrap()
        )
        .is_none()
    );
}
