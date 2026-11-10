use super::{Session, strips::RhythmStrip};
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Text, TextAlign, TextVerticalAlign};

fn lyrics() -> ParagraphStyle {
    ParagraphStyle::default()
        .with_align(TextAlign::Center)
        .with_vertical_align(TextVerticalAlign::Center)
}

#[compose]
pub(super) fn build_widgets(session: model!(Session), factor: u16) {
    let f = factor as i32;
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(4 * f),
            row_gap: 2 * f,
            align: AlignItems::Stretch,
            bg_color: ColorToken::Surface
        ) {
            Column (
                height: if factor == 1 { 40 } else { 28 * f },
                align: AlignItems::Stretch,
                bg_color: ${ if session.connected() { ColorToken::Success } else { ColorToken::SurfaceVariant } }
            ) {
                Text (
                    id: "prompter_chord",
                    text: ${ session.chord() },
                    width: Dimension::percent(100),
                    grow: 1.0,
                    font: super::font::CHORD,
                    font_size: if factor == 1 { 36 } else { 24 * factor },
                    text_color: ${ if session.connected() { ColorToken::Surface } else { ColorToken::OnSurface } },
                    paragraph: ParagraphStyle::label()
                )
            } on Tap { super::runtime::report(session.next_section()); }
            Text (
                id: "prompter_lyric",
                text: ${ session.lyric() },
                width: Dimension::percent(100),
                grow: 1.0,
                min_height: 24 * f,
                font_size: 12 * factor,
                text_color: ColorToken::OnSurface,
                paragraph: lyrics()
            )
            Row (height: 12 * f) {
                Text (
                    text: ${ session.next_chord() },
                    width: 24 * f,
                    height: 12 * f,
                    font_size: 12 * factor,
                    text_color: ColorToken::Primary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ session.next_lyric() },
                    grow: 1.0,
                    height: 12 * f,
                    font_size: 12 * factor,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (height: 4 * f) [
                RhythmStrip {
                    model: session.clone(),
                    strums: false,
                },
                IgnoreHitTest,
            ]
            View (height: 10 * f) [
                RhythmStrip {
                    model: session.clone(),
                    strums: true,
                },
                IgnoreHitTest,
            ]
            if factor > 1 {
                Row (height: 14 * f, column_gap: 2 * f) {
                    Text (
                        text: ${ session.tempo() },
                        width: 50 * f,
                        font_size: 12 * factor,
                        paragraph: ParagraphStyle::label()
                    )
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
                            font_size: 12 * factor,
                            text_color: ColorToken::OnPrimary,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap(super::runtime::toggle::callback(session.clone()))
                    Button (
                        width: 28 * f,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "STOP",
                            font_size: 12 * factor,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { session.stop(); }
                    Button (
                        width: 14 * f,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "-",
                            font_size: 12 * factor,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { super::runtime::report(session.adjust_tempo(-4)); }
                    Button (
                        width: 14 * f,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "+",
                            font_size: 12 * factor,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { super::runtime::report(session.adjust_tempo(4)); }
                }
            }
        }
    };
}
