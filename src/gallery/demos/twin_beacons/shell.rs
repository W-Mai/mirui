use super::input::{TwinKeyboardPlugin, move_model};
use super::render::surface_view;
use super::state::{TwinNodes, TwinSurface};
use super::style::{APRICOT, BG, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL, TEXT};
use crate::gallery::play::expeditions::{Direction4, ExpeditionHintWorkspace, ExpeditionUiState};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::twin::TwinModel;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

const MAP_CHAPTER_IDS: [&str; 6] = [
    "twin_map_chapter_0",
    "twin_map_chapter_1",
    "twin_map_chapter_2",
    "twin_map_chapter_3",
    "twin_map_chapter_4",
    "twin_map_chapter_5",
];
const MAP_LEVEL_IDS: [&str; 6] = [
    "twin_map_level_0",
    "twin_map_level_1",
    "twin_map_level_2",
    "twin_map_level_3",
    "twin_map_level_4",
    "twin_map_level_5",
];
const SUMMARY_LINE_IDS: [&str; 6] = [
    "twin_summary_line_0",
    "twin_summary_line_1",
    "twin_summary_line_2",
    "twin_summary_line_3",
    "twin_summary_line_4",
    "twin_summary_line_5",
];

#[compose]
fn build_widgets() {
    ui! {
        TwinSurface (id: "twin_surface", width: 480, height: 320, clip_children: true) {
            Text (
                "TWIN BEACONS",
                position: Position::Absolute,
                left: 15,
                top: 7,
                width: 220,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "twin_chapter",
                position: Position::Absolute,
                left: 238,
                top: 10,
                width: 136,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "twin_level",
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
                "BEACON A",
                position: Position::Absolute,
                left: 18,
                top: 44,
                width: 128,
                height: 13,
                font_size: 7,
                text_color: MINT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "BEACON B",
                position: Position::Absolute,
                left: 165,
                top: 44,
                width: 128,
                height: 13,
                font_size: 7,
                text_color: LAVENDER,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "SYNC ROUTE",
                position: Position::Absolute,
                left: 331,
                top: 62,
                width: 118,
                height: 14,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "One command\nTwo mirrored routes",
                position: Position::Absolute,
                left: 329,
                top: 94,
                width: 124,
                height: 29,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
            )
            Button (
                "↑",
                position: Position::Absolute,
                left: 366,
                top: 132,
                width: 40,
                height: 32,
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
                left: 324,
                top: 168,
                width: 40,
                height: 32,
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
                left: 366,
                top: 168,
                width: 40,
                height: 32,
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
                left: 408,
                top: 168,
                width: 40,
                height: 32,
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
                left: 327,
                top: 214,
                width: 58,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: FLOOR,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { TwinNodes::update(ctx.world, TwinModel::undo); }
            Button (
                "HINT",
                position: Position::Absolute,
                left: 391,
                top: 214,
                width: 62,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { TwinNodes::hint(ctx.world); }
            Text (
                "",
                id: "twin_status",
                position: Position::Absolute,
                left: 16,
                top: 216,
                width: 282,
                height: 21,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "twin_steps",
                position: Position::Absolute,
                left: 16,
                top: 245,
                width: 282,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "RESTART",
                position: Position::Absolute,
                left: 16,
                top: 276,
                width: 66,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { TwinNodes::update(ctx.world, TwinModel::restart); }
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 88,
                top: 276,
                width: 73,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { TwinNodes::open_map(ctx.world); }
            Button (
                "RULES",
                position: Position::Absolute,
                left: 167,
                top: 276,
                width: 55,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: FLOOR,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { TwinNodes::open_rules(ctx.world); }
            Button (
                "",
                id: "twin_next",
                position: Position::Absolute,
                left: 228,
                top: 276,
                width: 70,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap { TwinNodes::next(ctx.world); }
            Text (
                "WASD · Z UNDO · H HINT",
                position: Position::Absolute,
                left: 318,
                top: 278,
                width: 146,
                height: 21,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "twin_result",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(5, 12, 20, 244)
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
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "twin_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: MINT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "twin_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一条航路已经开放；当前最优记录会保留。",
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
                        ) on Tap { TwinNodes::update(ctx.world, TwinModel::restart); }
                        Button (
                            "查看航图",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::open_map(ctx.world); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: MINT,
                            pressed_color: LAVENDER,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap { TwinNodes::next(ctx.world); }
                    }
                }
            }
            View (
                id: "twin_map",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(5, 12, 20, 244)
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
                        ) on Tap { TwinNodes::close_map(ctx.world); }
                    }
                    Text (
                        "",
                        id: "twin_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "twin_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            id: "twin_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            id: "twin_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            id: "twin_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            id: "twin_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            id: "twin_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::set_map_chapter(ctx.world, 5); }
                    }
                    Text (
                        "",
                        id: "twin_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: MINT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        Button (
                            "01",
                            id: "twin_map_level_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 0); }
                        Button (
                            "02",
                            id: "twin_map_level_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 1); }
                        Button (
                            "03",
                            id: "twin_map_level_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 2); }
                        Button (
                            "04",
                            id: "twin_map_level_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 3); }
                        Button (
                            "05",
                            id: "twin_map_level_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 4); }
                        Button (
                            "06",
                            id: "twin_map_level_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { TwinNodes::select_map_level(ctx.world, 5); }
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
                id: "twin_rules",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(5, 12, 20, 244)
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
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "TWIN BEACONS · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: MINT,
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
                        ) on Tap { TwinNodes::close_map(ctx.world); }
                    }
                    Text (
                        "01  方向键同时移动两位探索者，撞墙的一位会停住。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  必须同时停在各自的双圆环信标；到站后仍会移动。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  后续章节会改变 B 站方向，屏幕右侧会完整标注。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  钥片由 A 取得；取到后的下一步起可通过闸门。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  用墙壁制造错位，再让两位探索者汇合。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
            View (
                id: "twin_briefing",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(5, 12, 20, 244)
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
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "twin_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: MINT,
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
                        id: "twin_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "留意 B 站的新指令方向；撞墙可制造相对位移。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "出现钥片时，先让 A 取得，再穿过两站闸门。",
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
                        normal_color: MINT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { TwinNodes::close_map(ctx.world); }
                }
            }
            View (
                id: "twin_summary",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(5, 12, 20, 248)
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
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 双子信标",
                        height: 25,
                        font_size: 13,
                        text_color: MINT,
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
                        id: "twin_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "twin_summary_line_5",
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
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: FLOOR,
                            pressed_color: MINT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { TwinNodes::open_map_chapter(ctx.world, 5); }
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
    app.add_plugin(TwinKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(TwinModel::default());
    app.world.insert_resource(ExpeditionUiState::default());
    app.world
        .insert_resource(ExpeditionHintWorkspace::default());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Twin Beacons node");
    app.world.insert_resource(TwinNodes {
        surface: find("twin_surface"),
        level: find("twin_level"),
        chapter: find("twin_chapter"),
        status: find("twin_status"),
        steps: find("twin_steps"),
        next: find("twin_next"),
        map: find("twin_map"),
        map_summary: find("twin_map_summary"),
        map_chapter_name: find("twin_map_chapter_name"),
        map_chapter_mechanic: find("twin_map_chapter_mechanic"),
        map_chapters: core::array::from_fn(|index| find(MAP_CHAPTER_IDS[index])),
        map_levels: core::array::from_fn(|index| find(MAP_LEVEL_IDS[index])),
        rules: find("twin_rules"),
        result: find("twin_result"),
        result_title: find("twin_result_title"),
        result_stats: find("twin_result_stats"),
        briefing: find("twin_briefing"),
        briefing_title: find("twin_briefing_title"),
        briefing_mechanic: find("twin_briefing_mechanic"),
        summary: find("twin_summary"),
        summary_lines: core::array::from_fn(|index| find(SUMMARY_LINE_IDS[index])),
    });
    TwinNodes::sync(&mut app.world);
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_twin_beacons.bin"))
        .bytes(
            "twin_beacons/save",
            |world| world.resource::<TwinModel>().map(TwinModel::encode_vec),
            |world, bytes| {
                if let Ok(model) = TwinModel::decode(bytes) {
                    world.insert_resource(model);
                }
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
