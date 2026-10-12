use super::{RequestError, SelectAll};
use crate::input::event::focus::FocusState;
use crate::input::event::input::{InputEvent, KEY_A, KEY_C, Key, KeyModifiers};
use crate::prelude::*;
use crate::ui::clipboard::{
    self, ClipboardData, ClipboardError, CopySelection, InspectSelection, SelectionState,
};

#[crate::component]
#[derive(Default)]
struct Paper {
    selected: bool,
    revision: u64,
}

#[crate::event]
fn choose(
    node: entity!(),
    _event: event!(SelectAll),
    papers: query!(mut Paper),
) -> Result<(), RequestError> {
    let paper = papers.get(node).ok_or(RequestError::Unavailable)?;
    paper.selected = true;
    paper.revision += 1;
    Ok(())
}

#[crate::event]
fn inspect(
    node: entity!(),
    _event: event!(InspectSelection),
    papers: query!(Paper),
) -> Result<SelectionState, ClipboardError> {
    let paper = papers.get(node).ok_or(ClipboardError::Unavailable)?;
    Ok(SelectionState::new(paper.selected, paper.revision))
}

#[crate::event]
fn export(_event: event!(CopySelection)) -> Result<ClipboardData, ClipboardError> {
    ClipboardData::text("paper")
}

#[crate::view(component = Paper, input = (choose::callback(), inspect::callback(), export::callback()))]
fn paper_view() {}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, Entity) {
    let mut app = App::headless(32, 32);
    app.with_widget(paper_view::view());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| ui! { View () [Paper::default()] });
    app.add_resource(FocusState {
        focused: Some(node),
    });
    (app, node)
}

fn key(app: &mut TestApp, code: u32, pressed: bool, repeat: bool, modifiers: KeyModifiers) {
    crate::input::event::deliver_input(
        &mut app.world,
        app.root,
        &InputEvent::Key {
            code,
            pressed,
            repeat: Some(repeat),
            modifiers: Some(modifiers),
        },
        0,
        32,
        32,
    );
}

#[test]
fn shortcuts_use_custom_component_requests_and_ignore_repeat_release_and_altgr() {
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
        let (mut app, node) = fixture();
        key(&mut app, KEY_A, true, false, modifiers);
        assert!(app.world.get::<Paper>(node).unwrap().selected);
        key(&mut app, KEY_C, true, false, modifiers);
        let write = clipboard::tests::take_write(&mut app.world).unwrap();
        assert_eq!(write.data().plain_text(), Ok("paper"));
        clipboard::complete(&mut app.world, &write.complete(Ok(())));
        for (pressed, repeat, mods) in [
            (false, false, modifiers),
            (true, true, modifiers),
            (
                true,
                false,
                KeyModifiers {
                    alt: true,
                    ..modifiers
                },
            ),
            (
                true,
                false,
                KeyModifiers {
                    shift: true,
                    ..modifiers
                },
            ),
        ] {
            key(&mut app, KEY_C, pressed, repeat, mods);
            assert!(clipboard::tests::take_write(&mut app.world).is_none());
        }
    }
}

#[test]
fn prevented_key_does_not_run_a_default_request() {
    let (mut app, node) = fixture();
    crate::input::event::runtime::install_entity(
        &mut app.world,
        node,
        Key::EVENT,
        prevent::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    key(
        &mut app,
        KEY_A,
        true,
        false,
        KeyModifiers {
            control: true,
            ..Default::default()
        },
    );
    assert!(!app.world.get::<Paper>(node).unwrap().selected);
}

#[crate::event]
fn prevent(_event: event!(Key)) -> bool {
    true
}

#[crate::event]
fn redirect(_event: event!(Key), target: Entity, focus: res!(mut FocusState)) -> bool {
    focus.focused = Some(target);
    false
}

#[test]
fn focus_changes_in_key_callbacks_do_not_redirect_the_default_request() {
    let (mut app, original) = fixture();
    let root = app.root.unwrap();
    let other = app.compose(root, |cx| ui! { View () [Paper::default()] });
    crate::input::event::runtime::install_entity(
        &mut app.world,
        original,
        Key::EVENT,
        redirect::callback::<crate::input::event::callback::NodeContext>(other),
    )
    .unwrap();
    key(
        &mut app,
        KEY_A,
        true,
        false,
        KeyModifiers {
            meta: true,
            ..Default::default()
        },
    );
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(other)
    );
    assert!(app.world.get::<Paper>(original).unwrap().selected);
    assert!(!app.world.get::<Paper>(other).unwrap().selected);
}

#[crate::event]
fn replace(_event: event!(Key), node: entity!(), commands: commands!()) -> bool {
    commands.insert(node, Paper::default());
    false
}

#[test]
fn provider_replacement_in_key_callbacks_invalidates_the_captured_default() {
    let (mut app, node) = fixture();
    crate::input::event::runtime::install_entity(
        &mut app.world,
        node,
        Key::EVENT,
        replace::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    key(
        &mut app,
        KEY_A,
        true,
        false,
        KeyModifiers {
            control: true,
            ..Default::default()
        },
    );
    assert!(!app.world.get::<Paper>(node).unwrap().selected);
}
