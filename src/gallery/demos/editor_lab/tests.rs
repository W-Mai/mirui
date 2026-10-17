use super::*;
use crate::core::model::BindType;
use crate::input::event::focus::FocusState;
use crate::input::event::input::{InputEvent, KEY_RETURN, PointerKind};
use crate::ui::widgets::{TextEditing, TextInput, TextSelection};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, Entity, <model::EditorLab as BindType>::Shared) {
    let mut app = App::headless(440, 760);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(crate::gallery::play::font::PlayFontPlugin);
    app.with_widget(editors::search_view::view())
        .with_widget(editors::command_view::view())
        .with_widget(editors::shared_view::view());
    let root = app.spawn_root().id();
    let model = app.add_model(model::EditorLab::default()).finish();
    app.compose(root, build_widgets(model.clone()));
    (app, root, model)
}

fn dispatch(app: &mut TestApp, root: Entity, target: Entity, event: InputEvent) {
    app.add_resource(FocusState {
        focused: Some(target),
    });
    crate::input::event::dispatch_input(&mut app.world, Some(root), &event, 0, 440, 760);
    crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
}

#[test]
fn clipboard_shortcuts_use_the_focused_shared_editors_selection() {
    use crate::input::event::input::{KEY_A, KEY_C, KEY_V, KEY_X, KeyModifiers};
    use crate::ui::clipboard;
    let (mut app, _, model) = fixture();
    let first = app.world.find_by_id("editor_shared_a").unwrap();
    let second = app.world.find_by_id("editor_shared_b").unwrap();
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(first);
    let second_selection = app.world.get::<TextEditing>(second).unwrap().selection();
    for code in [KEY_A, KEY_C] {
        crate::input::event::deliver_input(
            &mut app.world,
            app.root,
            &InputEvent::Key {
                code,
                pressed: true,
                modifiers: Some(KeyModifiers {
                    meta: true,
                    ..Default::default()
                }),
                repeat: Some(false),
            },
            0,
            440,
            760,
        );
    }
    let write = clipboard::tests::take_write(&mut app.world).unwrap();
    model
        .shared()
        .read(|text| assert_eq!(write.data().plain_text(), Ok(text)));
    assert_eq!(
        app.world.get::<TextEditing>(second).unwrap().selection(),
        second_selection
    );
    clipboard::complete(&mut app.world, &write.complete(Ok(())));
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert_eq!(model.clipboard_status(), "Copied");
    let original = model
        .shared()
        .read(|text| alloc::string::String::from(text));
    for code in [KEY_X, KEY_V] {
        crate::input::event::deliver_input(
            &mut app.world,
            app.root,
            &InputEvent::Key {
                code,
                pressed: true,
                modifiers: Some(KeyModifiers {
                    meta: true,
                    ..Default::default()
                }),
                repeat: Some(false),
            },
            0,
            440,
            760,
        );
        let response = match clipboard::take_request(&mut app.world).unwrap() {
            clipboard::ClipboardRequest::Write(write) => {
                assert_eq!(code, KEY_X);
                assert_eq!(write.data().plain_text(), Ok(original.as_str()));
                write.complete(Ok(()))
            }
            clipboard::ClipboardRequest::Read(read) => {
                assert_eq!(code, KEY_V);
                read.complete(clipboard::ClipboardData::text(&original))
            }
            clipboard::ClipboardRequest::DeferredRead(_, _) => panic!("unexpected native paste"),
        };
        clipboard::complete(&mut app.world, &response);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        model
            .shared()
            .read(|text| assert_eq!(text, if code == KEY_X { "" } else { &original }));
        assert_eq!(
            model.clipboard_status(),
            if code == KEY_X { "Cut" } else { "Pasted" }
        );
    }
}

#[test]
fn selection_toolbar_uses_the_target_editors_range_without_moving_focus() {
    let (mut app, _, model) = fixture();
    let first = app.world.find_by_id("editor_shared_a").unwrap();
    let second = app.world.find_by_id("editor_shared_b").unwrap();
    let button = app.world.find_by_id("editor_select_all").unwrap();
    let second_selection = app.world.get::<TextEditing>(second).unwrap().selection();
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(first);
    let content = model
        .shared()
        .read(|text| alloc::string::String::from(text));
    crate::ui::delivery::dispatch_gesture(
        &mut app.world,
        &crate::input::event::gesture::GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: button,
        },
        0,
    )
    .unwrap();
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(first)
    );
    assert_eq!(
        app.world.get::<TextEditing>(first).unwrap().selection(),
        TextSelection {
            anchor: 0,
            cursor: content.len()
        }
    );
    assert_eq!(
        app.world.get::<TextEditing>(second).unwrap().selection(),
        second_selection
    );
    model.shared().read(|text| assert_eq!(text, content));
}

#[test]
fn screen_keyboard_composes_into_custom_editors_without_changing_the_shared_preview() {
    let (mut app, _, model) = fixture();
    app.with_input_method(InputMethod::software((
        keyboard::session_changed::callback(model.keyboard()),
        keyboard::editing_key::callback(model.keyboard()),
    )));
    let first = app.world.find_by_id("editor_shared_a").unwrap();
    let second = app.world.find_by_id("editor_shared_b").unwrap();
    model.shared().set_text("").unwrap();
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(first);
    app.tick();
    app.render().unwrap();
    for (id, expected) in [("ime_n", "n"), ("ime_i", "ni"), ("ime_choose", "你")] {
        let target = app.world.find_by_id(id).unwrap();
        let rect = app.world.get::<crate::ui::ComputedRect>(target).unwrap().0;
        for event in [
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(4),
                y: rect.y + Fixed::from_int(4),
            },
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(4),
                y: rect.y + Fixed::from_int(4),
            },
        ] {
            crate::input::event::deliver_input(&mut app.world, app.root, &event, 1, 440, 760);
        }
        app.tick();
        app.render().unwrap();
        app.world
            .get::<TextEditing>(first)
            .unwrap()
            .read_display(|text| assert_eq!(text, expected));
        app.world
            .get::<TextEditing>(second)
            .unwrap()
            .read_display(|text| assert_eq!(text, if expected == "你" { "你" } else { "" }));
    }
}

#[test]
fn explicit_capabilities_and_appearance_take_precedence_over_attachment_defaults() {
    use super::editors::{CommandEditor, SearchEditor, SharedEditor};

    #[crate::compose]
    fn panel() -> [Entity; 3] {
        let search = ui! {
            SearchEditor () [
                TextEditing::default().with_text("explicit").unwrap(),
                TextInput::new().with_cursor_color(ColorToken::OnSurface),
            ]
        };
        let command = ui! {
            CommandEditor () [
                TextEditing::default().with_text("explicit").unwrap(),
                TextInput::new().with_cursor_color(ColorToken::OnSurface),
            ]
        };
        let shared = ui! {
            SharedEditor () [
                TextEditing::default().with_text("explicit").unwrap(),
                TextInput::new().with_cursor_color(ColorToken::OnSurface),
            ]
        };
        [search, command, shared]
    }

    let (mut app, root, _) = fixture();
    for entity in app.compose(root, panel()) {
        app.world
            .get::<TextEditing>(entity)
            .unwrap()
            .read(|text| assert_eq!(text, "explicit"));
        assert_eq!(
            app.world.get::<TextInput>(entity).unwrap().cursor_color,
            ColorToken::OnSurface.into()
        );
    }
}

#[test]
fn custom_attachments_share_input_behavior_and_preserve_document_identity() {
    let (mut app, root, model) = fixture();
    for id in [
        "editor_search",
        "editor_command",
        "editor_shared_a",
        "editor_shared_b",
    ] {
        let entity = app.world.find_by_id(id).unwrap();
        assert!(app.world.has::<TextInput>(entity));
        assert!(app.world.has::<TextEditing>(entity));
    }
    let search = app.world.find_by_id("editor_search").unwrap();
    app.render().unwrap();
    let rect = app.world.get::<crate::ui::ComputedRect>(search).unwrap().0;
    for (time, event) in [
        (
            1,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(4),
                y: rect.y + Fixed::from_int(4),
            },
        ),
        (
            2,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(4),
                y: rect.y + Fixed::from_int(4),
            },
        ),
        (3, TextCommitted::new("CAN").into()),
    ] {
        crate::input::event::deliver_input(&mut app.world, Some(root), &event, time, 440, 760);
    }
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(search)
    );
    assert_eq!(model.matches(), 1);
    app.tick();
    app.render_dirty().unwrap();
    for entity in app.world.query::<crate::ui::widgets::Text>().collect() {
        let text = app
            .world
            .get::<crate::ui::widgets::Text>(entity)
            .unwrap()
            .resolve(None);
        if model::TOOLS.contains(&text.as_ref()) {
            assert_eq!(
                crate::ui::branch::is_hidden_in_tree(&app.world, entity),
                text != "Canvas"
            );
        }
    }

    let command = app.world.find_by_id("editor_command").unwrap();
    dispatch(&mut app, root, command, TextCommitted::new("green").into());
    dispatch(
        &mut app,
        root,
        command,
        InputEvent::Key {
            code: KEY_RETURN,
            pressed: true,
            modifiers: None,
            repeat: None,
        },
    );
    assert!(model.accent());
    assert_eq!(model.output(), "Accent: green");
    assert!(model.command().is_empty());

    let first = app.world.find_by_id("editor_shared_a").unwrap();
    let second = app.world.find_by_id("editor_shared_b").unwrap();
    app.world
        .get_mut::<TextEditing>(first)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 0,
            cursor: 16,
        })
        .unwrap();
    dispatch(&mut app, root, first, TextCommitted::new("中🙂").into());
    app.world
        .get::<TextEditing>(second)
        .unwrap()
        .read(|text| assert_eq!(text, "中🙂"));
    assert_eq!(app.world.get::<TextEditing>(first).unwrap().cursor(), 7);
    assert_eq!(app.world.get::<TextEditing>(second).unwrap().cursor(), 0);
    app.render().unwrap();
    model.reset_shared().unwrap();
    app.render_dirty().unwrap();
    app.world
        .get::<TextEditing>(second)
        .unwrap()
        .read(|text| assert_eq!(text, "Hello from the document"));

    let rect = app.world.get::<crate::ui::ComputedRect>(first).unwrap().0;
    for (time, event) in [
        (
            10,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(4),
            },
        ),
        (
            11,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(4),
            },
        ),
        (12, TextCommitted::new(">").into()),
    ] {
        crate::input::event::deliver_input(&mut app.world, Some(root), &event, time, 440, 760);
    }
    assert_eq!(app.world.get::<TextEditing>(first).unwrap().cursor(), 1);
    assert_eq!(app.world.get::<TextEditing>(second).unwrap().cursor(), 0);
    app.world
        .get::<TextEditing>(second)
        .unwrap()
        .read(|text| assert_eq!(text, ">Hello from the document"));
}

#[test]
fn document_toolbar_preserves_editor_focus_and_accepts_subsequent_text() {
    let (mut app, root, model) = fixture();
    let editor = app.world.find_by_id("editor_shared_a").unwrap();
    let button = app.world.find_by_id("editor_replace").unwrap();
    app.render().unwrap();
    for (index, target) in [editor, button].into_iter().enumerate() {
        let rect = app.world.get::<crate::ui::ComputedRect>(target).unwrap().0;
        let x = rect.x + Fixed::from_int(4);
        let y = rect.y + Fixed::from_int(4);
        for (offset, event) in [
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y,
            },
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y,
            },
        ]
        .into_iter()
        .enumerate()
        {
            crate::input::event::deliver_input(
                &mut app.world,
                Some(root),
                &event,
                1 + index as u32 * 10 + offset as u32,
                440,
                760,
            );
        }
    }
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(editor)
    );
    model
        .shared()
        .read(|text| assert_eq!(text, "Hello from the document"));
    let event = TextCommitted::new("!").into();
    crate::input::event::deliver_input(&mut app.world, Some(root), &event, 30, 440, 760);
    model.shared().read(|text| assert!(text.contains('!')));
}
