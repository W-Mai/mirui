use super::input::{FoldKeyboardPlugin, move_model};
use super::render::surface_view;
use super::state::{FoldNodes, FoldSurface};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::{Direction4, ExpeditionHintWorkspace, ExpeditionUiState};
use crate::gallery::play::fold::FoldModel;
use crate::gallery::play::font::register_play_font;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

const MAP_CHAPTER_IDS: [&str; 6] = [
    "fold_map_chapter_0",
    "fold_map_chapter_1",
    "fold_map_chapter_2",
    "fold_map_chapter_3",
    "fold_map_chapter_4",
    "fold_map_chapter_5",
];
const MAP_LEVEL_IDS: [&str; 6] = [
    "fold_map_level_0",
    "fold_map_level_1",
    "fold_map_level_2",
    "fold_map_level_3",
    "fold_map_level_4",
    "fold_map_level_5",
];
const SUMMARY_LINE_IDS: [&str; 6] = [
    "fold_summary_line_0",
    "fold_summary_line_1",
    "fold_summary_line_2",
    "fold_summary_line_3",
    "fold_summary_line_4",
    "fold_summary_line_5",
];

#[compose]
fn build_widgets() {
    ui! {
        FoldSurface (id: "fold_surface", width: 480, height: 320, clip_children: true) {
            Text (
                "FOLDING ARK",
                position: Position::Absolute,
                left: 15,
                top: 7,
                width: 210,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "fold_chapter",
                position: Position::Absolute,
                left: 228,
                top: 10,
                width: 146,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "fold_level",
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
                "BRIDGE / SEALS",
                position: Position::Absolute,
                left: 369,
                top: 61,
                width: 84,
                height: 16,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "Roll across the chart.\nFragile tiles reject\nan upright hull.",
                position: Position::Absolute,
                left: 330,
                top: 98,
                width: 122,
                height: 50,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
            )
            Button (
                "↑",
                position: Position::Absolute,
                left: 367,
                top: 145,
                width: 40,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: FLOOR,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { move_model(ctx.world, Direction4::Up); }
            Button (
                "←",
                position: Position::Absolute,
                left: 325,
                top: 180,
                width: 40,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: FLOOR,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { move_model(ctx.world, Direction4::Left); }
            Button (
                "↓",
                position: Position::Absolute,
                left: 367,
                top: 180,
                width: 40,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: FLOOR,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { move_model(ctx.world, Direction4::Down); }
            Button (
                "→",
                position: Position::Absolute,
                left: 409,
                top: 180,
                width: 40,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: FLOOR,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { move_model(ctx.world, Direction4::Right); }
            Button (
                "UNDO",
                position: Position::Absolute,
                left: 328,
                top: 217,
                width: 58,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: FLOOR,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { FoldNodes::update(ctx.world, FoldModel::undo); }
            Button (
                "HINT",
                position: Position::Absolute,
                left: 392,
                top: 217,
                width: 61,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { FoldNodes::hint(ctx.world); }
            Text (
                "",
                id: "fold_status",
                position: Position::Absolute,
                left: 16,
                top: 260,
                width: 231,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "fold_steps",
                position: Position::Absolute,
                left: 16,
                top: 280,
                width: 231,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 252,
                top: 270,
                width: 56,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { FoldNodes::open_map(ctx.world); }
            Button (
                "RULES",
                position: Position::Absolute,
                left: 314,
                top: 270,
                width: 45,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { FoldNodes::open_rules(ctx.world); }
            Button (
                "RESET",
                position: Position::Absolute,
                left: 365,
                top: 270,
                width: 45,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { FoldNodes::update(ctx.world, FoldModel::restart); }
            Button (
                "",
                id: "fold_next",
                position: Position::Absolute,
                left: 416,
                top: 270,
                width: 50,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 6,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap { FoldNodes::next(ctx.world); }
            View (
                id: "fold_result",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "fold_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: LAVENDER,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "fold_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一片海域已经开放；当前最优记录会保留。",
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
                            normal_color: FLOOR,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::update(ctx.world, FoldModel::restart); }
                        Button (
                            "查看航图",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::open_map(ctx.world); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: LAVENDER,
                            pressed_color: MINT,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap { FoldNodes::next(ctx.world); }
                    }
                }
            }
            View (
                id: "fold_map",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                            normal_color: FLOOR,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::close_map(ctx.world); }
                    }
                    Text (
                        "",
                        id: "fold_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "fold_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            id: "fold_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            id: "fold_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            id: "fold_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            id: "fold_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            id: "fold_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::set_map_chapter(ctx.world, 5); }
                    }
                    Text (
                        "",
                        id: "fold_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: LAVENDER,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        Button (
                            "01",
                            id: "fold_map_level_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 0); }
                        Button (
                            "02",
                            id: "fold_map_level_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 1); }
                        Button (
                            "03",
                            id: "fold_map_level_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 2); }
                        Button (
                            "04",
                            id: "fold_map_level_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 3); }
                        Button (
                            "05",
                            id: "fold_map_level_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 4); }
                        Button (
                            "06",
                            id: "fold_map_level_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { FoldNodes::select_map_level(ctx.world, 5); }
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
                id: "fold_rules",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "FOLDING ARK · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: LAVENDER,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: FLOOR,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::close_map(ctx.world); }
                    }
                    Text (
                        "01  用方向键翻滚：直立占一格，横卧或纵卧占两格。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  所有占用格都要有支撑；裂纹板不能承受直立船体。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  经过金色徽记即可收集，收齐后才能直立归港。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  直立压 P 会切换桥梁；平躺经过不会触发。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  危险移动会被拦住，不计步数，可以直接改走其他方向。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
            View (
                id: "fold_briefing",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 244)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "fold_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: LAVENDER,
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
                        id: "fold_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "薄冰可以躺着经过，但不能直立；开关只响应直立船体。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "收齐全部金色徽记，再直立到达菱形归港点。",
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
                        normal_color: LAVENDER,
                        pressed_color: MINT,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { FoldNodes::close_map(ctx.world); }
                }
            }
            View (
                id: "fold_summary",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(6, 12, 21, 248)
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
                    border_color: LAVENDER,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 折叠方舟",
                        height: 25,
                        font_size: 13,
                        text_color: LAVENDER,
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
                        id: "fold_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "fold_summary_line_5",
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
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: LAVENDER,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { FoldNodes::open_map_chapter(ctx.world, 5); }
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
    app.add_plugin(FoldKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(FoldModel::default());
    app.world.insert_resource(ExpeditionUiState::default());
    app.world
        .insert_resource(ExpeditionHintWorkspace::default());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Folding Ark node");
    app.world.insert_resource(FoldNodes {
        surface: find("fold_surface"),
        level: find("fold_level"),
        chapter: find("fold_chapter"),
        status: find("fold_status"),
        steps: find("fold_steps"),
        next: find("fold_next"),
        map: find("fold_map"),
        map_summary: find("fold_map_summary"),
        map_chapter_name: find("fold_map_chapter_name"),
        map_chapter_mechanic: find("fold_map_chapter_mechanic"),
        map_chapters: core::array::from_fn(|index| find(MAP_CHAPTER_IDS[index])),
        map_levels: core::array::from_fn(|index| find(MAP_LEVEL_IDS[index])),
        rules: find("fold_rules"),
        result: find("fold_result"),
        result_title: find("fold_result_title"),
        result_stats: find("fold_result_stats"),
        briefing: find("fold_briefing"),
        briefing_title: find("fold_briefing_title"),
        briefing_mechanic: find("fold_briefing_mechanic"),
        summary: find("fold_summary"),
        summary_lines: core::array::from_fn(|index| find(SUMMARY_LINE_IDS[index])),
    });
    FoldNodes::sync(&mut app.world);
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_folding_ark.bin"))
        .bytes(
            "folding_ark/save",
            |world| world.resource::<FoldModel>().map(FoldModel::encode_vec),
            |world, bytes| {
                if let Ok(model) = FoldModel::decode(bytes) {
                    world.insert_resource(model);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
