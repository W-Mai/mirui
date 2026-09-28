#[cfg(feature = "audio")]
use crate::audio::AudioHandle;
use crate::gallery::play::marble::{MarbleModel, Page};
use crate::prelude::*;
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

fn marble_status(page: Page, paused: bool, add_mode: bool) -> &'static str {
    match page {
        Page::Play if paused => "HOLD · PHYSICS PAUSED",
        Page::Play => "LIVE · DRAG EMPTY SPACE TO TILT",
        Page::Edit if add_mode => "EDIT · TAP EMPTY SPACE TO ADD",
        Page::Edit => "EDIT · DRAG A PAD TO MOVE",
        Page::Scenes => "SCENES · CHOOSE A LITTLE WORLD",
        Page::Settings => "SETTINGS · SESSION ONLY",
    }
}

fn marble_readout(
    page: Page,
    pad_letter: u8,
    gravity: Fixed,
    hits: u32,
    radius: Fixed,
    bounce: Fixed,
) -> MarbleReadout {
    MarbleReadout {
        page,
        pad_letter,
        gravity,
        hits,
        radius,
        bounce,
    }
}

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
        match self.page {
            Page::Play => write!(
                out,
                "PAD {} · {:.2} g · {} HITS",
                self.pad_letter as char,
                self.gravity.to_f32(),
                self.hits
            ),
            Page::Edit => write!(
                out,
                "PAD {} · RADIUS {} · BOUNCE {:.2}",
                self.pad_letter as char,
                self.radius.to_int(),
                self.bounce.to_f32()
            ),
            Page::Scenes => out.write_str("PRESETS RESET LAYOUT AND MARBLES"),
            Page::Settings => out.write_str("GRAVITY · TRAILS · FEEDBACK"),
        }
    }
}

fn marble_count(count: usize) -> &'static str {
    const LABELS: [&str; 9] = [
        "0 MARBLES",
        "1 MARBLE",
        "2 MARBLES",
        "3 MARBLES",
        "4 MARBLES",
        "5 MARBLES",
        "6 MARBLES",
        "7 MARBLES",
        "8 MARBLES",
    ];
    LABELS[count]
}

fn pad_count(count: usize) -> &'static str {
    const LABELS: [&str; 7] = [
        "0 PADS", "1 PAD", "2 PADS", "3 PADS", "4 PADS", "5 PADS", "6 PADS",
    ];
    LABELS[count]
}

fn nav_color(page: Page, active: Page) -> Color {
    if page == active {
        Color::rgb(217, 248, 138)
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

#[compose(bind(model))]
pub(super) fn build_widgets(
    model: MarbleModel,
    #[cfg(feature = "audio")] audio: Option<AudioHandle>,
) {
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

    ui! {
        Column (
            width: 480,
            height: 320,
            bg_color: Color::rgb(23, 34, 28)
        ) {
            Row (
                height: 29,
                padding: symmetric_padding(4, 10),
                align: AlignItems::Center,
                column_gap: 6,
                bg_color: Color::rgb(31, 45, 35)
            ) {
                Text (
                    "MARBLE PLAY",
                    grow: 1.0,
                    height: 21,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    text: ${ if model.paused() { "PLAY" } else { "HOLD" } },
                    text_capacity: 4,
                    id: "marble_pause",
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 22,
                    font_size: 9,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.toggle_pause(); }
                Button (
                    id: "marble_audio",
                    size: ButtonSize::Compact,
                    width: 50,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    text: ${ audio_label(&audio_label_state) },
                    text_capacity: 5,
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
                Button (
                    text: ${ if model.recording() { "DONE" } else if model.looping() { "STOP" } else { "REC" } },
                    text_capacity: 4,
                    id: "marble_record",
                    size: ButtonSize::Compact,
                    width: 44,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(238, 172, 139),
                    text_color: TEXT,
                    visible: ${ audio_visible(&audio_state) },
                    border_radius: 6
                ) on Tap { model.toggle_recording(); }
                Button (
                    "+",
                    size: ButtonSize::Compact,
                    width: 54,
                    height: 22,
                    font_size: 12,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.drop_ball(); }
            }
            Row (
                height: 23,
                padding: symmetric_padding(3, 14),
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Text (
                    text: ${ marble_status(model.page(), model.paused(), model.add_mode()) },
                    text_capacity: 32,
                    id: "marble_status",
                    grow: 1.0,
                    height: 17,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ marble_count(model.ball_count()) },
                    text_capacity: 9,
                    id: "marble_marble_count",
                    width: 74,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    text: ${ pad_count(model.pad_count()) },
                    text_capacity: 6,
                    id: "marble_pad_count",
                    width: 52,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
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
            Row (
                height: 35,
                padding: symmetric_padding(5, 14),
                align: AlignItems::Center,
                column_gap: 6
            ) {
                Text (
                    text: ${
                        marble_readout(
                            model.page(),
                            model.selected_letter(),
                            model.gravity().to_fixed(),
                            model.hits(),
                            model.selected_radius().to_fixed(),
                            model.selected_bounce().to_fixed(),
                        )
                    },
                    text_capacity: 34,
                    id: "marble_readout",
                    grow: 1.0,
                    height: 22,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                if ${ model.page() == Page::Edit } {
                    Button (
                        "PROPS",
                        id: "marble_properties",
                        size: ButtonSize::Compact,
                        width: 52,
                        height: 23,
                        font_size: 8,
                        normal_color: Color::rgb(34, 47, 37),
                        pressed_color: Color::rgb(217, 248, 138),
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.set_inspector(true); }
                    Button (
                        "ADD",
                        id: "marble_add",
                        size: ButtonSize::Compact,
                        width: 46,
                        height: 23,
                        font_size: 8,
                        normal_color: Color::rgb(34, 47, 37),
                        pressed_color: Color::rgb(217, 248, 138),
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.toggle_add_mode(); }
                    Button (
                        "REMOVE",
                        id: "marble_remove",
                        size: ButtonSize::Compact,
                        width: 62,
                        height: 23,
                        font_size: 8,
                        normal_color: Color::rgb(34, 47, 37),
                        pressed_color: Color::rgb(238, 172, 139),
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.remove_selected(); }
                }
            }
            Row (
                height: 34,
                padding: symmetric_padding(4, 5),
                column_gap: 6,
                bg_color: Color::rgb(31, 45, 35)
            ) {
                Button (
                    "PLAY",
                    id: "marble_nav_play",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Play) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Play) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Play); }
                Button (
                    "EDIT",
                    id: "marble_nav_edit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Edit) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Edit) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Edit); }
                Button (
                    "SCENES",
                    id: "marble_nav_scenes",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Scenes) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Scenes) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Scenes); }
                Button (
                    "SETTINGS",
                    id: "marble_nav_settings",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Settings) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Settings) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Settings); }
            }
            marble_inspector (model)
        }
    };
}
