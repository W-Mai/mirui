use super::{Session, strips::RhythmStrip};
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Text, TextAlign};

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
pub(super) fn build_widgets(session: model!(Session), factor: u16) {
    let f = factor as i32;
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(6 * f),
            row_gap: 3 * f,
            bg_color: ColorToken::Surface
        ) {
            Row (height: 12 * f, align: AlignItems::Center) {
                Text (
                    "CHORD / LIVE",
                    grow: 1.0,
                    height: 12 * f,
                    font_size: 6 * factor,
                    text_color: ColorToken::Primary,
                    paragraph: label()
                )
                Text (
                    text: ${ session.tempo() },
                    width: 44 * f,
                    height: 12 * f,
                    font_size: 6 * factor,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: label()
                )
            }
            if factor > 1 {
                Row (height: 12 * f) {
                    Text (
                        text: ${ session.title() },
                        grow: 1.0,
                        font_size: 8 * factor,
                        height: 12 * f,
                        paragraph: label()
                    )
                    Text (
                        text: ${ session.progress() },
                        width: 58 * f,
                        font_size: 6 * factor,
                        height: 12 * f,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: label()
                    )
                }
            }
            Row (height: 30 * f, align: AlignItems::Center) {
                Text (
                    id: "prompter_chord",
                    text: ${ session.chord() },
                    grow: 1.0,
                    height: 30 * f,
                    font_size: 26 * factor,
                    text_color: ColorToken::OnSurface,
                    paragraph: label()
                )
                Text (
                    text: ${ session.next_chord() },
                    width: 32 * f,
                    height: 20 * f,
                    font_size: 12 * factor,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: label()
                )
            } on Tap { super::runtime::report(session.next_section()); }
            Column (grow: 1.0, min_height: 16 * f, row_gap: 2 * f, clip_children: true) {
                Text (
                    id: "prompter_lyric",
                    text: ${ session.lyric() },
                    grow: 1.0,
                    min_height: 12 * f,
                    font_size: 10 * factor,
                    text_color: ColorToken::OnSurface,
                    paragraph: label()
                )
                Text (
                    text: ${ session.next_lyric() },
                    height: 12 * f,
                    font_size: 7 * factor,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: label()
                )
            }
            View (height: 8 * f) [
                RhythmStrip {
                    model: session.clone(),
                    strums: false,
                },
                IgnoreHitTest,
            ]
            View (height: 15 * f) [
                RhythmStrip {
                    model: session.clone(),
                    strums: true,
                },
                IgnoreHitTest,
            ]
            Row (height: 13 * f, column_gap: 3 * f) {
                Button (
                    id: "prompter_play",
                    grow: 1.0,
                    padding: Padding::all(1),
                    normal_color: ColorToken::Primary,
                    border_radius: 0
                ) {
                    Text (
                        text: ${ if session.playing() { "PAUSE" } else { "PLAY" } },
                        grow: 1.0,
                        height: 11 * f,
                        font_size: 6 * factor,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap(super::runtime::toggle::callback(session.clone()))
                Button (
                    width: 22 * f,
                    padding: Padding::all(1),
                    normal_color: ColorToken::SurfaceVariant,
                    border_radius: 0
                ) {
                    Text (
                        "STOP",
                        font_size: 5 * factor,
                        height: 11 * f,
                        grow: 1.0,
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { session.stop(); }
                Button (
                    width: 13 * f,
                    padding: Padding::all(1),
                    normal_color: ColorToken::SurfaceVariant,
                    border_radius: 0
                ) {
                    Text (
                        "-",
                        font_size: 7 * factor,
                        height: 11 * f,
                        grow: 1.0,
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { super::runtime::report(session.adjust_tempo(-4)); }
                Button (
                    width: 13 * f,
                    padding: Padding::all(1),
                    normal_color: ColorToken::SurfaceVariant,
                    border_radius: 0
                ) {
                    Text (
                        "+",
                        font_size: 7 * factor,
                        height: 11 * f,
                        grow: 1.0,
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { super::runtime::report(session.adjust_tempo(4)); }
            }
        }
    };
}
