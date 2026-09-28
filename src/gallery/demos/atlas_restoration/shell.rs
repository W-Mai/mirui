use super::input::{PictureKeyboardPlugin, surface_gesture};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::surface_view;
use super::state::{PictureNodes, PictureSurface};
use super::style::{APRICOT, BG, CELL, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::ExpeditionUiState;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

const ROW_IDS: [&str; 10] = [
    "pic_row_0",
    "pic_row_1",
    "pic_row_2",
    "pic_row_3",
    "pic_row_4",
    "pic_row_5",
    "pic_row_6",
    "pic_row_7",
    "pic_row_8",
    "pic_row_9",
];
const COLUMN_IDS: [&str; 10] = [
    "pic_col_0",
    "pic_col_1",
    "pic_col_2",
    "pic_col_3",
    "pic_col_4",
    "pic_col_5",
    "pic_col_6",
    "pic_col_7",
    "pic_col_8",
    "pic_col_9",
];
const MAP_CHAPTER_IDS: [&str; 6] = [
    "picture_map_chapter_0",
    "picture_map_chapter_1",
    "picture_map_chapter_2",
    "picture_map_chapter_3",
    "picture_map_chapter_4",
    "picture_map_chapter_5",
];
const MAP_LEVEL_IDS: [&str; 6] = [
    "picture_map_level_0",
    "picture_map_level_1",
    "picture_map_level_2",
    "picture_map_level_3",
    "picture_map_level_4",
    "picture_map_level_5",
];
const SUMMARY_LINE_IDS: [&str; 6] = [
    "picture_summary_line_0",
    "picture_summary_line_1",
    "picture_summary_line_2",
    "picture_summary_line_3",
    "picture_summary_line_4",
    "picture_summary_line_5",
];

#[compose]
fn build_widgets() {
    ui! {
        PictureSurface (id: "picture_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "ATLAS RESTORATION",
                position: Position::Absolute,
                left: 15,
                top: 7,
                width: 230,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "picture_chapter",
                position: Position::Absolute,
                left: 247,
                top: 10,
                width: 127,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "picture_level",
                position: Position::Absolute,
                left: 383,
                top: 9,
                width: 81,
                height: 17,
                font_size: 9,
                text_color: MINT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_0",
                position: Position::Absolute,
                left: 25,
                top: 82,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_1",
                position: Position::Absolute,
                left: 25,
                top: 100,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_2",
                position: Position::Absolute,
                left: 25,
                top: 118,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_3",
                position: Position::Absolute,
                left: 25,
                top: 136,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_4",
                position: Position::Absolute,
                left: 25,
                top: 154,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_5",
                position: Position::Absolute,
                left: 25,
                top: 172,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_6",
                position: Position::Absolute,
                left: 25,
                top: 190,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_7",
                position: Position::Absolute,
                left: 25,
                top: 208,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_8",
                position: Position::Absolute,
                left: 25,
                top: 226,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_9",
                position: Position::Absolute,
                left: 25,
                top: 244,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_col_0",
                position: Position::Absolute,
                left: 125,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_1",
                position: Position::Absolute,
                left: 143,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_2",
                position: Position::Absolute,
                left: 161,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_3",
                position: Position::Absolute,
                left: 179,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_4",
                position: Position::Absolute,
                left: 197,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_5",
                position: Position::Absolute,
                left: 215,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_6",
                position: Position::Absolute,
                left: 233,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_7",
                position: Position::Absolute,
                left: 251,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_8",
                position: Position::Absolute,
                left: 269,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_9",
                position: Position::Absolute,
                left: 287,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "OBSERVATION TOOLS",
                position: Position::Absolute,
                left: 335,
                top: 63,
                width: 119,
                height: 15,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "FILL",
                id: "picture_fill",
                position: Position::Absolute,
                left: 336,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: MINT,
                pressed_color: MINT,
                text_color: BG,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, |model| model.set_tool(PictureTool::Fill)); }
            Button (
                "MARK",
                id: "picture_mark",
                position: Position::Absolute,
                left: 398,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, |model| model.set_tool(PictureTool::Mark)); }
            Button (
                "UNDO",
                position: Position::Absolute,
                left: 336,
                top: 126,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::undo); }
            Button (
                "REVEAL ONE",
                position: Position::Absolute,
                left: 336,
                top: 161,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::reveal_hint); }
            Button (
                "CHECK",
                position: Position::Absolute,
                left: 336,
                top: 196,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::check); }
            Text (
                "",
                id: "picture_progress",
                position: Position::Absolute,
                left: 334,
                top: 232,
                width: 121,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "picture_status",
                position: Position::Absolute,
                left: 18,
                top: 274,
                width: 126,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 150,
                top: 269,
                width: 79,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::open_map(ctx.world); }
            Button (
                "RULES",
                position: Position::Absolute,
                left: 235,
                top: 269,
                width: 82,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::open_rules(ctx.world); }
            Button (
                "RESET",
                position: Position::Absolute,
                left: 324,
                top: 269,
                width: 61,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::restart); }
            Button (
                "",
                id: "picture_next",
                position: Position::Absolute,
                left: 391,
                top: 269,
                width: 75,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap { PictureNodes::next(ctx.world); }
            View (
                id: "picture_result",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 48,
                    top: 66,
                    width: 384,
                    height: 188,
                    padding: Padding::all(18),
                    row_gap: 13,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "picture_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "picture_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一幅图已经开放；提示不会影响关卡解锁。",
                        height: 18,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Row (height: 34, column_gap: 8) {
                        Button (
                            "重新挑战",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::update(ctx.world, PictureModel::restart); }
                        Button (
                            "查看航图",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::open_map(ctx.world); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: APRICOT,
                            pressed_color: LAVENDER,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap { PictureNodes::next(ctx.world); }
                    }
                }
            }
            View (
                id: "picture_map",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 5,
                    top: 36,
                    width: 470,
                    height: 239,
                    padding: Padding::all(12),
                    row_gap: 5,
                    bg_color: PANEL,
                    border_color: GRID,
                    border_width: 1,
                    border_radius: 8
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "EXPEDITION MAP",
                            grow: 1.0,
                            height: 22,
                            font_size: 13,
                            text_color: TEXT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::close_map(ctx.world); }
                    }
                    Text (
                        "",
                        id: "picture_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "picture_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            id: "picture_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            id: "picture_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            id: "picture_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            id: "picture_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            id: "picture_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 5); }
                    }
                    Text (
                        "",
                        id: "picture_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        Button (
                            "01",
                            id: "picture_map_level_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 0); }
                        Button (
                            "02",
                            id: "picture_map_level_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 1); }
                        Button (
                            "03",
                            id: "picture_map_level_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 2); }
                        Button (
                            "04",
                            id: "picture_map_level_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 3); }
                        Button (
                            "05",
                            id: "picture_map_level_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 4); }
                        Button (
                            "06",
                            id: "picture_map_level_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 5); }
                    }
                    Text (
                        "6 CHAPTERS × 6 LEVELS · COMPLETE TO UNLOCK",
                        height: 16,
                        font_size: 7,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
            View (
                id: "picture_rules",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 30,
                    top: 54,
                    width: 420,
                    height: 212,
                    padding: Padding::all(16),
                    row_gap: 9,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "ATLAS RESTORATION · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: APRICOT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::close_map(ctx.world); }
                    }
                    Text (
                        "01  边缘数字表示这一行或列中连续填色段的长度。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  例如 2 1：填两格，至少空一格，再填一格。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  选择填色或标空，按住拖动；同一笔画可一次撤销。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  青点是固定观测点，不可修改；空格不必全部标叉。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  方向键选格，空格使用当前工具，X 键标空。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
            View (
                id: "picture_briefing",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 42,
                    top: 58,
                    width: 396,
                    height: 204,
                    padding: Padding::all(17),
                    row_gap: 10,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "picture_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "新机制解锁 · 先理解规则，再启程",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "picture_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "尺寸增加时优先处理大数字与完整行。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "段与段之间至少隔一个空格；青点观测不可修改。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Button (
                        "开始这一章 →",
                        size: ButtonSize::Compact,
                        width: 190,
                        height: 31,
                        font_size: 9,
                        normal_color: APRICOT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { PictureNodes::close_map(ctx.world); }
                }
            }
            View (
                id: "picture_summary",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 248)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 36,
                    top: 38,
                    width: 408,
                    height: 244,
                    padding: Padding::all(15),
                    row_gap: 5,
                    bg_color: PANEL,
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 星图修复局",
                        height: 25,
                        font_size: 13,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "36 关全部完成；每章记录保留，可随时重访。",
                        height: 16,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "picture_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_5",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 27, column_gap: 5) {
                        Button (
                            "01",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 5); }
                    }
                }
            }
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(PictureKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(PictureModel::default());
    app.world.insert_resource(ExpeditionUiState::default());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Atlas Restoration node");
    app.world.insert_resource(PictureNodes {
        surface: find("picture_surface"),
        level: find("picture_level"),
        chapter: find("picture_chapter"),
        status: find("picture_status"),
        progress: find("picture_progress"),
        next: find("picture_next"),
        fill: find("picture_fill"),
        mark: find("picture_mark"),
        map: find("picture_map"),
        map_summary: find("picture_map_summary"),
        map_chapter_name: find("picture_map_chapter_name"),
        map_chapter_mechanic: find("picture_map_chapter_mechanic"),
        map_chapters: core::array::from_fn(|index| find(MAP_CHAPTER_IDS[index])),
        map_levels: core::array::from_fn(|index| find(MAP_LEVEL_IDS[index])),
        rules: find("picture_rules"),
        result: find("picture_result"),
        result_title: find("picture_result_title"),
        result_stats: find("picture_result_stats"),
        briefing: find("picture_briefing"),
        briefing_title: find("picture_briefing_title"),
        briefing_mechanic: find("picture_briefing_mechanic"),
        summary: find("picture_summary"),
        summary_lines: core::array::from_fn(|index| find(SUMMARY_LINE_IDS[index])),
        row_clues: core::array::from_fn(|index| find(ROW_IDS[index])),
        column_clues: core::array::from_fn(|index| find(COLUMN_IDS[index])),
    });
    PictureNodes::sync(&mut app.world);
}
