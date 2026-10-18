use crate::input::event::input::{KEY_BACKSPACE, KEY_RETURN};
use crate::prelude::*;
use crate::ui::widgets::{Button, Text};

#[crate::model]
#[derive(Default)]
pub(super) struct Keyboard {
    session: Option<TextInputSession>,
    #[observe]
    spelling: &'static str,
}

#[crate::model]
impl Keyboard {
    fn set_session(&mut self, session: Option<TextInputSession>) {
        if self.session != session {
            self.session = session;
            self.spelling = "";
        }
    }
    fn session(&self) -> Option<TextInputSession> {
        self.session
    }
    fn type_n(&mut self) {
        self.spelling = "n";
    }
    fn type_i(&mut self) {
        self.spelling = "ni";
    }
    fn choose(&mut self) {
        self.spelling = "";
    }
    fn backspace(&mut self) {
        self.spelling = match self.spelling {
            "ni" => "n",
            _ => "",
        };
    }
}

#[crate::event]
pub(super) fn session_changed(event: event!(TextInputChanged), keyboard: model!(Keyboard)) {
    keyboard.set_session(event.state.map(|state| state.session));
}

#[crate::event]
pub(super) fn editing_key(
    event: event!(Key),
    keyboard: model!(Keyboard),
    commands: commands!(),
) -> bool {
    if !event.pressed {
        return false;
    }
    if event.code == KEY_BACKSPACE
        && !keyboard.spelling().is_empty()
        && let Some(session) = keyboard.session()
    {
        keyboard.backspace();
        let text = keyboard.spelling();
        commands
            .text_input()
            .preedit(session, text, Some(text.len()..text.len()));
        return true;
    }
    if event.code == KEY_RETURN
        && keyboard.spelling() == "ni"
        && let Some(session) = keyboard.session()
    {
        keyboard.choose();
        commands.text_input().commit(session, "你");
        return true;
    }
    false
}

#[crate::compose]
pub(super) fn panel(keyboard: model!(Keyboard)) -> Entity {
    ui! {
        Column (row_gap: 8) [
            PointerFocus::Preserve,
        ] {
            Text (
                "04 / SCREEN KEYBOARD",
                height: 20,
                font_size: 12,
                text_color: ColorToken::Primary
            )
            Row (height: 32, column_gap: 8) {
                Button (id: "ime_software", width: 112) [
                    Text::label("Screen"),
                ] on Tap { commands!().text_input().set_mode(TextInputMode::Software); }
                Button (id: "ime_native", width: 112) [
                    Text::label("System"),
                ] on Tap { commands!().text_input().set_mode(TextInputMode::Native); }
                Text (text: ${ keyboard.spelling() }, grow: 1.0)
            }
            Row (height: 36, column_gap: 8) {
                Button (id: "ime_n", width: 48) [
                    Text::label("n"),
                ] on Tap {
                    if let Some(session) = keyboard.session() {
                        keyboard.type_n();
                        commands!().text_input().preedit(session, "n", Some(1..1));
                    }
                }
                Button (id: "ime_i", width: 48) [
                    Text::label("i"),
                ] on Tap {
                    if keyboard.spelling() == "n" && let Some(session) = keyboard.session() {
                        keyboard.type_i();
                        commands!().text_input().preedit(session, "ni", Some(2..2));
                    }
                }
                Button (id: "ime_choose", width: 64, visible: ${ keyboard.spelling() == "ni" }) [
                    Text::label("你"),
                ] on Tap {
                    if keyboard.spelling() == "ni" && let Some(session) = keyboard.session() {
                        keyboard.choose();
                        commands!().text_input().commit(session, "你");
                    }
                }
                Button (id: "ime_delete", width: 96) [
                    Text::label("Delete"),
                ] on Tap {
                    if let Some(session) = keyboard.session() {
                        if keyboard.spelling().is_empty() {
                            commands!()
                                .text_input()
                                .key(
                                    session,
                                    Key {
                                        physical: None,
                                        code: KEY_BACKSPACE,
                                        pressed: true,
                                        modifiers: None,
                                        repeat: None,
                                    },
                                );
                        } else {
                            keyboard.backspace();
                            let text = keyboard.spelling();
                            commands!()
                                .text_input()
                                .preedit(session, text, Some(text.len()..text.len()));
                        }
                    }
                }
            }
        }
    }
}
