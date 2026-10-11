use super::*;
use crate::core::model::BindType;
use crate::input::event::focus::FocusState;
use crate::input::event::input::{InputEvent, KEY_RETURN, PointerKind};
use crate::ui::widgets::{TextEditing, TextInput, TextSelection};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, Entity, <model::EditorLab as BindType>::Shared) {
    let mut app = App::headless(440, 640);
    app.with_default_widgets().with_default_systems();
    app.with_widget(editors::search_view::view())
        .with_widget(editors::command_view::view())
        .with_widget(editors::shared_view::view());
    let root = app.spawn_root().id();
    let model = app.add_model(model::EditorLab::default());
    app.compose(root, build_widgets(model.clone()));
    (app, root, model)
}

fn dispatch(app: &mut TestApp, root: Entity, target: Entity, event: InputEvent) {
    app.add_resource(FocusState {
        focused: Some(target),
    });
    crate::input::event::dispatch_input(&mut app.world, Some(root), &event, 0, 440, 640);
    crate::__macro_support::flush_entity_events(&mut app.world).unwrap();
}

#[test]
fn explicit_capabilities_and_appearance_take_precedence_over_attachment_defaults() {
    use super::editors::{CommandEditor, SearchEditor, SharedEditor};

    #[crate::compose]
    fn panel() -> [Entity; 3] {
        let search = ui! { SearchEditor() [
            TextEditing::default().with_text("explicit").unwrap(),
            TextInput::new().with_cursor_color(ColorToken::OnSurface),
        ] };
        let command = ui! { CommandEditor() [
            TextEditing::default().with_text("explicit").unwrap(),
            TextInput::new().with_cursor_color(ColorToken::OnSurface),
        ] };
        let shared = ui! { SharedEditor() [
            TextEditing::default().with_text("explicit").unwrap(),
            TextInput::new().with_cursor_color(ColorToken::OnSurface),
        ] };
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
        crate::input::event::deliver_input(&mut app.world, Some(root), &event, time, 440, 640);
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
        crate::input::event::deliver_input(&mut app.world, Some(root), &event, time, 440, 640);
    }
    assert_eq!(app.world.get::<TextEditing>(first).unwrap().cursor(), 1);
    assert_eq!(app.world.get::<TextEditing>(second).unwrap().cursor(), 0);
    app.world
        .get::<TextEditing>(second)
        .unwrap()
        .read(|text| assert_eq!(text, ">Hello from the document"));
}
