use super::geometry::text_path;
use super::style::{BACKGROUND, BORDER, CYAN, MUTED, PANEL, TEXT, bitmap_line};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Text, TextAlign};

#[compose]
pub fn build_widgets(path: PathId) {
    ui! {
        Column (
            id: "compact_curve_text_shell",
            grow: 1.0,
            padding: Padding::all(6),
            row_gap: 4,
            bg_color: BACKGROUND
        ) {
            Row (height: 14, align: AlignItems::Center, column_gap: 4) {
                View (width: 4, height: 10, bg_color: CYAN, border_radius: 2)
                Text (
                    "CURVE TEXT",
                    grow: 1.0,
                    height: 14,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: TEXT,
                    paragraph: bitmap_line(TextAlign::Start)
                )
            }
            View (id: "compact_curve_text_stage", grow: 1.0, clip_children: true) {
                Text (
                    id: "compact_curve_text_primary",
                    "MIRUI RIDES THE WAVE    MIRUI RIDES THE WAVE    ",
                    path: text_path(path, Fixed::ZERO),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    font: FontToken::Default,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: bitmap_line(TextAlign::Start)
                ) [
                    IgnoreHitTest,
                ]
            }
            View (
                id: "compact_curve_text_footer",
                width: Dimension::percent(100),
                height: 18,
                padding: Padding {
                    top: Dimension::px(3),
                    right: Dimension::px(10),
                    bottom: Dimension::px(3),
                    left: Dimension::px(10),
                },
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 8
            ) [
                IgnoreHitTest,
            ] {
                Text (
                    id: "compact_curve_text_footer_label",
                    "AUTO LOOP",
                    grow: 1.0,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: MUTED,
                    paragraph: bitmap_line(TextAlign::Center)
                ) [
                    IgnoreHitTest,
                ]
            }
        }
    };
}
