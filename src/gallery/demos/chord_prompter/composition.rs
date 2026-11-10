use super::{Session, strips::RhythmStrip};
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Text, TextAlign, TextVerticalAlign};

fn lyrics() -> ParagraphStyle {
    ParagraphStyle::default()
        .with_align(TextAlign::Center)
        .with_vertical_align(TextVerticalAlign::Center)
}

fn compact(width: Fixed, height: Fixed) -> bool {
    width < Fixed::from_int(256) || height < Fixed::from_int(200)
}

#[compose]
pub(super) fn build_widgets(session: model!(Session)) {
    ui! {
        Column (
            grow: 1.0,
            container: true,
            padding: Padding::all(4),
            row_gap: 2,
            align: AlignItems::Stretch,
            bg_color: ColorToken::Surface
        ) {
            Column (
                height: @(width, height) { if compact(width, height) { 40 } else { 56 } },
                align: AlignItems::Stretch,
                bg_color: ${ if session.connected() { ColorToken::Success } else { ColorToken::SurfaceVariant } }
            ) {
                Text (
                    id: "prompter_chord",
                    text: ${ session.chord() },
                    width: Dimension::percent(100),
                    grow: 1.0,
                    font: super::font::CHORD,
                    font_size: @(width, height) { if compact(width, height) { 36_u16 } else { 48_u16 } },
                    text_color: ${ if session.connected() { ColorToken::Surface } else { ColorToken::OnSurface } },
                    paragraph: ParagraphStyle::label()
                )
            } on Tap { super::runtime::report(session.next_section()); }
            Text (
                id: "prompter_lyric",
                text: ${ session.lyric() },
                width: Dimension::percent(100),
                grow: 1.0,
                min_height: 24,
                font_size: @(width, height) { if compact(width, height) { 12_u16 } else { 24_u16 } },
                text_color: ColorToken::OnSurface,
                paragraph: lyrics()
            )
            Row (height: 12) {
                Text (
                    text: ${ session.next_chord() },
                    width: 24,
                    height: 12,
                    font_size: 12,
                    text_color: ColorToken::Primary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    text: ${ session.next_lyric() },
                    grow: 1.0,
                    height: 12,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (height: 4) [
                RhythmStrip {
                    model: session.clone(),
                    strums: false,
                },
                IgnoreHitTest,
            ]
            View (height: 10) [
                RhythmStrip {
                    model: session.clone(),
                    strums: true,
                },
                IgnoreHitTest,
            ]
            Row (
                id: "prompter_controls",
                height: 20,
                column_gap: 2,
                visible: @(width, height) { !compact(width, height) }
            ) {
                    Text (
                        text: ${ session.tempo() },
                        width: 50,
                        font_size: 12,
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
                            font_size: 12,
                            text_color: ColorToken::OnPrimary,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap(super::runtime::toggle::callback(session.clone()))
                    Button (
                        width: 28,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "STOP",
                            font_size: 12,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { session.stop(); }
                    Button (
                        width: 14,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "-",
                            font_size: 12,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { super::runtime::report(session.adjust_tempo(-4)); }
                    Button (
                        width: 14,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "+",
                            font_size: 12,
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
