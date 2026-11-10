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
        ) on SizeChanged { session.resize(event.new.width, event.new.height); } {
            View (
                id: "prompter_connection", position: Position::Absolute,
                left: 1, top: 1, width: 3, height: 3,
                bg_color: ${ if session.connected() { ColorToken::Success } else { ColorToken::OnSurfaceVariant } }
            ) [ IgnoreHitTest ]
            Column (grow: 1.0, min_height: 66, clip_children: true) {
                Column (
                    id: "prompter_current_card",
                    position: Position::Absolute, top: 0,
                    left: ${ (session.stage_width() - super::motion::card_width(session.stage_width())) / 2 },
                    width: ${ super::motion::card_width(session.stage_width()) }, height: Dimension::percent(100),
                    align: AlignItems::Stretch,
                    transform: ${ super::motion::card_transform(session.bar_phase(), session.next_chord() != "END", session.stage_width(), 0) }
                ) {
                    Column (
                        height: @(width, height) { if compact(width, height) { 40 } else { 56 } },
                        align: AlignItems::Stretch
                    ) {
                        Text (
                            id: "prompter_chord", text: ${ session.chord() },
                            width: Dimension::percent(100), grow: 1.0,
                            font: super::font::CHORD,
                            font_size: ${ super::motion::chord_size(session.bar_phase(), session.next_chord() != "END", session.large(), 0) },
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    } on Tap { super::runtime::report(session.next_section()); }
                    Text (
                        id: "prompter_lyric", text: ${ session.lyric() },
                        width: Dimension::percent(100), grow: 1.0, min_height: 24,
                        font_size: @(width, height) { if compact(width, height) { 12_u16 } else { 24_u16 } },
                        text_color: ColorToken::OnSurface, paragraph: lyrics()
                    )
                }
                Column (
                    id: "prompter_previous_card",
                    position: Position::Absolute, top: 0,
                    left: ${ (session.stage_width() - super::motion::card_width(session.stage_width())) / 2 },
                    width: ${ super::motion::card_width(session.stage_width()) }, height: Dimension::percent(100),
                    align: AlignItems::Stretch,
                    visible: ${ !session.previous_chord().is_empty() },
                    transform: ${ super::motion::card_transform(session.bar_phase(), session.next_chord() != "END", session.stage_width(), -1) }
                ) {
                    Column (
                        height: @(width, height) { if compact(width, height) { 40 } else { 56 } },
                        align: AlignItems::Stretch
                    ) {
                        Text (
                            id: "prompter_previous_chord", text: ${ session.previous_chord() },
                            width: Dimension::percent(100), grow: 1.0,
                            font: super::font::CHORD,
                            font_size: ${ super::motion::chord_size(session.bar_phase(), session.next_chord() != "END", session.large(), -1) },
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    }
                    Text (
                        id: "prompter_previous_lyric", text: ${ session.previous_lyric() },
                        width: Dimension::percent(100), grow: 1.0, min_height: 24,
                        font_size: @(width, height) { if compact(width, height) { 12_u16 } else { 24_u16 } },
                        text_color: ColorToken::OnSurfaceVariant, paragraph: lyrics()
                    )
                }
                Column (
                    id: "prompter_next_card",
                    position: Position::Absolute, top: 0,
                    left: ${ (session.stage_width() - super::motion::card_width(session.stage_width())) / 2 },
                    width: ${ super::motion::card_width(session.stage_width()) }, height: Dimension::percent(100),
                    align: AlignItems::Stretch,
                    visible: ${ session.next_chord() != "END" },
                    transform: ${ super::motion::card_transform(session.bar_phase(), session.next_chord() != "END", session.stage_width(), 1) }
                ) {
                    Column (
                        height: @(width, height) { if compact(width, height) { 40 } else { 56 } },
                        align: AlignItems::Stretch
                    ) {
                        Text (
                            id: "prompter_next_chord", text: ${ session.next_chord() },
                            width: Dimension::percent(100), grow: 1.0,
                            font: super::font::CHORD,
                            font_size: ${ super::motion::chord_size(session.bar_phase(), session.next_chord() != "END", session.large(), 1) },
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    }
                    Text (
                        id: "prompter_next_lyric", text: ${ session.next_lyric() },
                        width: Dimension::percent(100), grow: 1.0, min_height: 24,
                        font_size: @(width, height) { if compact(width, height) { 12_u16 } else { 24_u16 } },
                        text_color: ColorToken::OnSurface, paragraph: lyrics()
                    )
                }
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
