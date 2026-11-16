#[cfg(feature = "audio")]
use crate::audio::AudioHandle;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::marble::{MarbleModel, Page};
use crate::prelude::*;
use crate::tr;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

#[cfg(feature = "audio")]
use super::audio::submit_pad_sound;
use super::audio::{audio_label, audio_visible};
use super::board::marble_play_page;
use super::inspector::marble_inspector;
use super::scenes::marble_scenes_page;
use super::settings::marble_settings_page;
use super::style::{MUTED, TEXT, symmetric_padding};

const BAR: Color = Color::rgb(31, 45, 35);
const CONTROL: Color = Color::rgb(45, 61, 48);
const LIME: Color = Color::rgb(217, 248, 138);
const INK: Color = Color::rgb(23, 34, 28);
const CORAL: Color = Color::rgb(238, 172, 139);

/// The status line: a toast while one shows, otherwise what the page does.
fn marble_status(page: Page, paused: bool, add_mode: bool, toast: Phrase) -> Phrase {
    if !toast.en.is_empty() {
        return toast;
    }
    match page {
        Page::Play if paused => tr!(en: "PAUSED", zh: "已暂停"),
        Page::Play => tr!(en: "DRAG TO TILT · TAP A PAD", zh: "拖动改变重力 · 点一下音盘"),
        Page::Edit if add_mode => {
            tr!(en: "TAP EMPTY SPACE TO PLACE A PAD", zh: "点空白处放一个音盘")
        }
        Page::Edit => tr!(en: "DRAG A PAD TO MOVE IT", zh: "拖动音盘来移动"),
        Page::Scenes => tr!(
            en: "A SCENE RESETS PADS AND MARBLES",
            zh: "换场景会重置音盘和弹珠"
        ),
        Page::Settings => tr!(en: "CHANGES APPLY AT ONCE", zh: "设置立即生效"),
    }
}

/// The right side of the status line: the selected pad, on the pages that
/// have one.
struct MarbleReadout {
    page: Page,
    pad_letter: u8,
    gravity: Fixed,
    hits: u32,
    radius: Fixed,
    bounce: Fixed,
}

impl fmt::Display for MarbleReadout {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pad = tr!(en: "PAD", zh: "音盘");
        let letter = self.pad_letter as char;
        let zh = locale() == Locale::ZhCn;
        match self.page {
            Page::Play if zh => write!(
                out,
                "{pad} {letter} · {:.2} g · {} 次",
                self.gravity.to_f32(),
                self.hits
            ),
            Page::Play => write!(
                out,
                "{pad} {letter} · {:.2} g · {} {}",
                self.gravity.to_f32(),
                self.hits,
                if self.hits == 1 { "HIT" } else { "HITS" }
            ),
            Page::Edit => write!(
                out,
                "{pad} {letter} · {} {} · {} {:.2}",
                tr!(en: "SIZE", zh: "大小"),
                self.radius.to_int(),
                tr!(en: "BOUNCE", zh: "弹性"),
                self.bounce.to_f32()
            ),
            Page::Scenes | Page::Settings => Ok(()),
        }
    }
}

fn nav_color(page: Page, active: Page) -> Color {
    if page == active {
        LIME
    } else {
        Color::rgb(34, 47, 37)
    }
}

fn nav_text_color(page: Page, active: Page) -> Color {
    if page == active {
        Color::rgb(48, 69, 41)
    } else {
        TEXT
    }
}

#[cfg(feature = "audio")]
type MarbleAudio = Option<AudioHandle>;

#[cfg(not(feature = "audio"))]
type MarbleAudio = ();

#[compose]
fn compose_tabs(model: model!(MarbleModel)) -> Entity {
    ui! {
        Button (
            tr!(en : "PLAY", zh : "运行"),
            id: "marble_nav_play",
            size: ButtonSize::Compact,
            width: 40,
            height: 24,
            font_size: 10,
            normal_color: ${ nav_color(model.page(), Page::Play) },
            pressed_color: LIME,
            text_color: ${ nav_text_color(model.page(), Page::Play) },
            border_radius: 6
        ) on Tap { model.set_page(Page::Play); }
    };
    ui! {
        Button (
            tr!(en : "EDIT", zh : "编辑"),
            id: "marble_nav_edit",
            size: ButtonSize::Compact,
            width: 40,
            height: 24,
            font_size: 10,
            normal_color: ${ nav_color(model.page(), Page::Edit) },
            pressed_color: LIME,
            text_color: ${ nav_text_color(model.page(), Page::Edit) },
            border_radius: 6
        ) on Tap { model.set_page(Page::Edit); }
    };
    ui! {
        Button (
            tr!(en : "SCENES", zh : "场景"),
            id: "marble_nav_scenes",
            size: ButtonSize::Compact,
            width: 52,
            height: 24,
            font_size: 10,
            normal_color: ${ nav_color(model.page(), Page::Scenes) },
            pressed_color: LIME,
            text_color: ${ nav_text_color(model.page(), Page::Scenes) },
            border_radius: 6
        ) on Tap { model.set_page(Page::Scenes); }
    };
    ui! {
        Button (
            tr!(en : "SETTINGS", zh : "设置"),
            id: "marble_nav_settings",
            size: ButtonSize::Compact,
            width: 62,
            height: 24,
            font_size: 10,
            normal_color: ${ nav_color(model.page(), Page::Settings) },
            pressed_color: LIME,
            text_color: ${ nav_text_color(model.page(), Page::Settings) },
            border_radius: 6
        ) on Tap { model.set_page(Page::Settings); }
    }
}

#[compose]
fn compose_header(model: model!(MarbleModel), audio: MarbleAudio) -> Entity {
    #[cfg(feature = "audio")]
    let audio_state = audio.as_ref().and_then(AudioHandle::state_signal);
    #[cfg(not(feature = "audio"))]
    let audio_state = false;
    #[cfg(feature = "audio")]
    let audio_label_state = audio_state.clone();
    #[cfg(not(feature = "audio"))]
    let audio_label_state = audio_state;
    #[cfg(feature = "audio")]
    let audio_button_state = audio_state.clone();
    #[cfg(not(feature = "audio"))]
    let audio_button_state = audio_state;
    #[cfg(not(feature = "audio"))]
    let _ = &audio;

    ui! {
        Row (
            height: 28,
            padding: symmetric_padding(2, 6),
            align: AlignItems::Center,
            column_gap: 3,
            bg_color: BAR
        ) {
            Text (
                tr!(en : "MARBLE PLAY", zh : "弹珠乐园"),
                width: 66,
                height: 14,
                font_size: 10,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            compose_tabs (model)
            View (grow: 1.0, height: 24)
            Button (
                id: "marble_audio",
                size: ButtonSize::Compact,
                width: 44,
                height: 24,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: LIME,
                pressed_text_color: INK,
                text_color: TEXT,
                text: ${ audio_label(&audio_label_state) },
                capacity: 8,
                visible: ${ audio_visible(&audio_button_state) },
                border_radius: 6
            ) on Tap {
                #[cfg(not(feature = "audio"))]
                let _ = &model;
                #[cfg(feature = "audio")]
                if let Some(audio) = &audio {
                    let enable = audio.state().is_some_and(|state| state.muted);
                    if audio.set_muted(!enable) && enable {
                        submit_pad_sound(
                            audio,
                            model.selected_pitch(),
                            model.selected_timbre(),
                            190,
                            0,
                        );
                    }
                }
            }
            if ${ model.page() == Page::Edit } {
                Button (
                    tr!(en : "ADJUST", zh : "调整"),
                    id: "marble_properties",
                    size: ButtonSize::Compact,
                    width: 50,
                    height: 24,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: LIME,
                    pressed_text_color: INK,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.set_inspector(true); }
                Button (
                    tr!(en : "+ PAD", zh : "+ 音盘"),
                    id: "marble_add",
                    size: ButtonSize::Compact,
                    width: 46,
                    height: 24,
                    font_size: 10,
                    normal_color: ${ if model.add_mode() { LIME } else { CONTROL } },
                    pressed_color: LIME,
                    pressed_text_color: INK,
                    text_color: ${ if model.add_mode() { INK } else { TEXT } },
                    border_radius: 6
                ) on Tap { model.toggle_add_mode(); }
                Button (
                    tr!(en : "REMOVE", zh : "移除"),
                    id: "marble_remove",
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 24,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CORAL,
                    pressed_text_color: INK,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.remove_selected(); }
            }
            if ${ model.page() != Page::Edit } {
                Button (
                    text: ${ if model.paused() { "▶" } else { "Ⅱ" } },
                    capacity: 4,
                    id: "marble_pause",
                    size: ButtonSize::Compact,
                    width: 28,
                    height: 24,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: LIME,
                    pressed_text_color: INK,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.toggle_pause(); }
                Button (
                    text: ${
                        if model.recording() {
                            tr!(en : "DONE", zh : "完成")
                        } else if model.looping() {
                            tr!(en : "STOP", zh : "停止")
                        } else {
                            tr!(en : "● REC", zh : "● 录制")
                        }
                    },
                    capacity: 12,
                    id: "marble_record",
                    size: ButtonSize::Compact,
                    width: 46,
                    height: 24,
                    font_size: 10,
                    normal_color: ${ if model.recording() { CORAL } else { CONTROL } },
                    pressed_color: CORAL,
                    pressed_text_color: INK,
                    text_color: ${ if model.recording() { INK } else { TEXT } },
                    visible: ${ audio_visible(&audio_state) },
                    border_radius: 6
                ) on Tap { model.toggle_recording(); }
                Button (
                    tr!(en : "+ MARBLE", zh : "+ 弹珠"),
                    id: "marble_drop",
                    size: ButtonSize::Compact,
                    width: 58,
                    height: 24,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: LIME,
                    pressed_text_color: INK,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.drop_ball(); }
            }
        }
    }
}

#[compose]
fn compose_status(model: model!(MarbleModel)) -> Entity {
    ui! {
        Row (
            height: 28,
            padding: symmetric_padding(2, 12),
            align: AlignItems::Center,
            column_gap: 8
        ) {
            Text (
                text: ${ marble_status(model.page(), model.paused(), model.add_mode(), model.toast()) },
                capacity: 48,
                id: "marble_status",
                grow: 1.0,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    MarbleReadout {
                        page: model.page(),
                        pad_letter: model.selected_letter(),
                        gravity: model.gravity().to_fixed(),
                        hits: model.hits(),
                        radius: model.selected_radius().to_fixed(),
                        bounce: model.selected_bounce().to_fixed(),
                    }
                },
                capacity: 48,
                id: "marble_readout",
                width: 196,
                height: 14,
                font_size: 10,
                text_color: LIME,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
        }
    }
}

#[compose]
pub(super) fn build_widgets(
    model: model!(MarbleModel),
    #[cfg(feature = "audio")] audio: Option<AudioHandle>,
) {
    #[cfg(not(feature = "audio"))]
    let audio = ();

    ui! {
        Column (
            width: 480,
            height: 320,
            bg_color: INK
        ) {
            compose_header (model, audio)
            match ${ model.page() } {
                Page :: Play | Page :: Edit => {
                    marble_play_page (model)
                }
                Page :: Scenes => {
                    marble_scenes_page (model)
                }
                Page :: Settings => {
                    marble_settings_page (model)
                }
            }
            compose_status (model)
            marble_inspector (model)
        }
    };
}
