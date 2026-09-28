use super::board::{LumenBoard, board_tap};
use super::runtime::LumenNodes;
use super::style::{
    ACCENT, BACKGROUND, CONTROL, CONTROL_PRESSED, HEADER, MUTED, PANEL, PANEL_BORDER, SUCCESS,
    TEXT, padding,
};
use crate::gallery::play::lumen::LumenModel;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

#[compose]
pub(super) fn build_widgets() {
    ui! {
        Column (width: 480, height: 320, bg_color: BACKGROUND) {
            Row (
                height: 35,
                padding: padding(7, 12),
                align: AlignItems::Center,
                column_gap: 8,
                bg_color: HEADER
            ) {
                View (width: 13, height: 13, bg_color: ACCENT, border_radius: 7)
                Text (
                    "LUMEN LAB",
                    grow: 1.0,
                    height: 21,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "PUZZLE 01 / 05",
                    id: "lumen_puzzle",
                    width: 110,
                    height: 20,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            Row (
                height: 28,
                padding: padding(4, 19),
                align: AlignItems::Center,
                column_gap: 8
            ) {
                Text (
                    "第一束光",
                    id: "lumen_level_name",
                    width: 164,
                    height: 20,
                    font_size: 12,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "先让光向上，再送向右边。",
                    id: "lumen_level_subtitle",
                    grow: 1.0,
                    height: 18,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            Row (height: 204, padding: padding(0, 12), column_gap: 11) {
                LumenBoard (
                    id: "lumen_board",
                    width: 282,
                    height: 204,
                    clip_children: true
                ) [
                    TouchAction::None,
                ] on Tap { board_tap(ctx.world, ctx.entity, ctx.event); }
                {
                    Text (
                        "1",
                        id: "lumen_mirror_1",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "2",
                        id: "lumen_mirror_2",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "3",
                        id: "lumen_mirror_3",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "4",
                        id: "lumen_mirror_4",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "5",
                        id: "lumen_mirror_5",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "6",
                        id: "lumen_mirror_6",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "7",
                        id: "lumen_mirror_7",
                        position: Position::Absolute,
                        width: 7,
                        height: 8,
                        font_size: 6,
                        text_color: Color::rgb(187, 203, 192),
                        paragraph: ParagraphStyle::label()
                    )
                }
                Column (width: 163, height: 204, row_gap: 6) {
                    Column (
                        height: 102,
                        padding: padding(9, 14),
                        row_gap: 3,
                        bg_color: PANEL,
                        border_color: PANEL_BORDER,
                        border_width: 1,
                        border_radius: 9
                    ) {
                        Text (
                            "等待点亮",
                            id: "lumen_status",
                            height: 20,
                            font_size: 11,
                            text_color: TEXT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Text (
                            "FOLLOW THE LIGHT",
                            id: "lumen_status_subtitle",
                            height: 13,
                            font_size: 7,
                            text_color: MUTED,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        View (height: 1, bg_color: Color::rgb(66, 82, 79))
                        Row (grow: 1.0, align: AlignItems::Center, column_gap: 4) {
                            Text (
                                "00",
                                id: "lumen_moves",
                                width: 42,
                                height: 35,
                                font_size: 24,
                                text_color: ACCENT,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                            )
                            Text (
                                "次旋转",
                                grow: 1.0,
                                height: 18,
                                font_size: 8,
                                text_color: MUTED,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                            )
                            Column (width: 43, height: 38) {
                                Text (
                                    "0 / 5",
                                    id: "lumen_completed",
                                    height: 21,
                                    font_size: 12,
                                    text_color: Color::rgb(193, 214, 197),
                                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                                )
                                Text (
                                    "关卡已点亮",
                                    height: 14,
                                    font_size: 7,
                                    text_color: MUTED,
                                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                                )
                            }
                        }
                    }
                    Button (
                        "提示一步",
                        size: ButtonSize::Compact,
                        width: 163,
                        height: 32,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 8
                    ) on Tap { LumenNodes::update(ctx.world, LumenModel::reveal_hint); }
                    Button (
                        "重新摆放",
                        size: ButtonSize::Compact,
                        width: 163,
                        height: 32,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 8
                    ) on Tap { LumenNodes::update(ctx.world, LumenModel::reset); }
                    Text (
                        "镜片只在两种方向间切换",
                        id: "lumen_hint_note",
                        grow: 1.0,
                        font_size: 7,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
            View (height: 15)
            Row (
                height: 38,
                padding: padding(6, 12),
                column_gap: 7,
                bg_color: Color::rgb(20, 30, 27)
            ) {
                Button (
                    "关卡",
                    size: ButtonSize::Compact,
                    width: 93,
                    height: 26,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { LumenNodes::update(ctx.world, LumenModel::open_levels); }
                Button (
                    "撤销",
                    id: "lumen_undo",
                    size: ButtonSize::Compact,
                    width: 93,
                    height: 26,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { LumenNodes::update(ctx.world, LumenModel::undo); }
                Button (
                    "追光 开",
                    id: "lumen_scan",
                    size: ButtonSize::Compact,
                    width: 113,
                    height: 26,
                    font_size: 10,
                    normal_color: ACCENT,
                    pressed_color: CONTROL_PRESSED,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { LumenNodes::update(ctx.world, LumenModel::toggle_scan); }
                Button (
                    "下一关",
                    id: "lumen_next",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: CONTROL_PRESSED,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { LumenNodes::update(ctx.world, LumenModel::next_level); }
            }
            View (
                id: "lumen_levels_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(8, 14, 13, 224)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 24,
                    top: 39,
                    width: 432,
                    height: 242,
                    padding: Padding::all(12),
                    row_gap: 5,
                    bg_color: PANEL,
                    border_color: ACCENT,
                    border_width: 1,
                    border_radius: 10
                ) {
                    Row (height: 25, align: AlignItems::Center) {
                        Text (
                            "五封写给光的信",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: ACCENT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 24,
                            font_size: 12,
                            normal_color: CONTROL,
                            pressed_color: CONTROL_PRESSED,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { LumenNodes::update(ctx.world, LumenModel::close_levels); }
                    }
                    Text (
                        "可以自由选关；只需把光送进圆环。",
                        height: 17,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "01   第一束光",
                        size: ButtonSize::Compact,
                        width: 408,
                        height: 28,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { LumenNodes::update(ctx.world, |model| model.select_level(0)); }
                    Button (
                        "02   折返航线",
                        size: ButtonSize::Compact,
                        width: 408,
                        height: 28,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { LumenNodes::update(ctx.world, |model| model.select_level(1)); }
                    Button (
                        "03   绕过岛屿",
                        size: ButtonSize::Compact,
                        width: 408,
                        height: 28,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { LumenNodes::update(ctx.world, |model| model.select_level(2)); }
                    Button (
                        "04   交错的光",
                        size: ButtonSize::Compact,
                        width: 408,
                        height: 28,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { LumenNodes::update(ctx.world, |model| model.select_level(3)); }
                    Button (
                        "05   最后一公里",
                        size: ButtonSize::Compact,
                        width: 408,
                        height: 28,
                        font_size: 10,
                        normal_color: CONTROL,
                        pressed_color: CONTROL_PRESSED,
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { LumenNodes::update(ctx.world, |model| model.select_level(4)); }
                    Text (
                        "✓",
                        id: "lumen_level_check_1",
                        position: Position::Absolute,
                        left: 388,
                        top: 66,
                        width: 18,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "✓",
                        id: "lumen_level_check_2",
                        position: Position::Absolute,
                        left: 388,
                        top: 99,
                        width: 18,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "✓",
                        id: "lumen_level_check_3",
                        position: Position::Absolute,
                        left: 388,
                        top: 132,
                        width: 18,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "✓",
                        id: "lumen_level_check_4",
                        position: Position::Absolute,
                        left: 388,
                        top: 165,
                        width: 18,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "✓",
                        id: "lumen_level_check_5",
                        position: Position::Absolute,
                        left: 388,
                        top: 198,
                        width: 18,
                        height: 18,
                        font_size: 10,
                        text_color: SUCCESS,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
        }
    };
}
