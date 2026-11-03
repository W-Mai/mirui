use super::launcher::Launcher;
use crate::input::event::focus::FocusState;
use crate::input::event::input::{KEY_BACKSPACE, KEY_RETURN};
use crate::prelude::*;

pub(super) const EDIT_CAPACITY: usize = 64;

#[derive(Clone, Copy)]
pub(super) enum Edit {
    Insert(u8),
    Backspace,
    Submit,
}

impl Edit {
    fn text(byte: u8) -> &'static str {
        const ASCII: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
        let index = usize::from(byte - b' ');
        &ASCII[index..index + 1]
    }
}

#[component]
pub(super) struct WifiEditor(pub u64);

pub(super) fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    state: super::model::DisplayStateHandle,
) {
    app.with_input_method(InputMethod::software((
        session_changed::callback(state.launcher()),
        super::input::keyboard::callback(state.clone()),
    )));
    app.add_system(focus::system(state.launcher()));
    app.add_system(edit::system(state.launcher()));
}

#[event]
fn session_changed(
    event: event!(TextInputChanged),
    launcher: model!(Launcher),
    focus: res!(FocusState),
    editors: query!(WifiEditor),
) {
    let current = launcher.active()
        && launcher.wifi_keyboard()
        && focus
            .focused
            .and_then(|node| editors.get(node))
            .is_some_and(|editor| editor.0 == launcher.wifi_epoch());
    launcher.text_session(event.state.filter(|_| current).map(|state| state.session));
}

#[system(order = Animation)]
fn focus(
    launcher: model!(Launcher),
    focus: res!(mut FocusState),
    editors: query!(Entity, WifiEditor),
) {
    let owned = focus
        .focused
        .is_some_and(|node| editors.get(node).is_some());
    let target = if launcher.active() && launcher.wifi_keyboard() {
        editors
            .into_iter()
            .find_map(|(node, editor)| (editor.0 == launcher.wifi_epoch()).then_some(node))
    } else {
        None
    };
    if target.is_some() || owned {
        focus.focused = target;
    }
}

#[system(order = Animation)]
fn edit(
    launcher: model!(Launcher),
    focus: res!(FocusState),
    editors: query!(WifiEditor),
    commands: commands!(),
) {
    if !launcher.active()
        || !launcher.wifi_keyboard()
        || !focus
            .focused
            .and_then(|node| editors.get(node))
            .is_some_and(|editor| editor.0 == launcher.wifi_epoch())
    {
        return;
    }
    if let Some(session) = launcher.input_session()
        && let Some(edit) = launcher.input_edit()
    {
        launcher.finish_input_edit();
        match edit {
            Edit::Insert(byte) => commands.text_input().commit(session, Edit::text(byte)),
            Edit::Backspace | Edit::Submit => commands.text_input().key(
                session,
                Key {
                    code: if matches!(edit, Edit::Submit) {
                        KEY_RETURN
                    } else {
                        KEY_BACKSPACE
                    },
                    pressed: true,
                    physical: None,
                    modifiers: None,
                    repeat: Some(false),
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wifi_compact_edits_resolve_every_printable_ascii_byte_without_owned_payloads() {
        assert_eq!(core::mem::size_of::<Edit>(), 2);
        for byte in b' '..=b'~' {
            assert_eq!(Edit::text(byte).as_bytes(), &[byte]);
        }
    }
}
