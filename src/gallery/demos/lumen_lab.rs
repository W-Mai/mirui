extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::lumen::{LEVEL_COUNT, LumenModel, MirrorOrientation, Trace};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

const BACKGROUND: Color = Color::rgb(25, 33, 34);
const HEADER: Color = Color::rgb(29, 39, 39);
const BOARD: Color = Color::rgb(21, 29, 32);
const TILE: Color = Color::rgb(32, 44, 46);
const PANEL: Color = Color::rgb(38, 49, 54);
const PANEL_BORDER: Color = Color::rgb(58, 71, 74);
const CONTROL: Color = Color::rgb(42, 57, 51);
const CONTROL_PRESSED: Color = Color::rgb(246, 214, 135);
const ACCENT: Color = Color::rgb(246, 214, 135);
const LIGHT: Color = Color::rgb(247, 239, 196);
const SUCCESS: Color = Color::rgb(189, 225, 154);
const TEXT: Color = Color::rgb(225, 234, 225);
const MUTED: Color = Color::rgb(148, 169, 159);

pub const VIEWPORT: (u16, u16) = (480, 320);

#[derive(crate::Component, Default)]
struct LumenBoard;

#[derive(Clone, Copy)]
struct LumenNodes {
    board: Entity,
    level_name: Entity,
    level_subtitle: Entity,
    puzzle: Entity,
    status: Entity,
    status_subtitle: Entity,
    moves: Entity,
    completed: Entity,
    hint_note: Entity,
    undo: Entity,
    scan: Entity,
    next: Entity,
    modal: Entity,
    mirror_labels: [Entity; 7],
    level_checks: [Entity; LEVEL_COUNT],
}

impl LumenNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut LumenModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<LumenModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::VISUAL)
            && let Some(board) = world.resource::<Self>().map(|nodes| nodes.board)
        {
            world.invalidate_visual(board);
        }
        if changes.contains(ChangeSet::MODEL) {
            Self::sync(world);
        }
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<LumenModel>() else {
            return;
        };
        let level = model.level();
        let level_name = level.name;
        let level_subtitle = level.subtitle;
        let level_index = model.level_index();
        let mirror_count = level.mirrors.len();
        let mirror_positions = core::array::from_fn::<_, 7, _>(|index| {
            level
                .mirrors
                .get(index)
                .map(|mirror| mirror.position)
                .unwrap_or_default()
        });
        let solved = model.trace().solved;
        let moves = model.moves();
        let completed = model.completed_count();
        let hint = model.hint();
        let scan = model.scan();
        let levels_open = model.levels_open();
        let can_undo = model.can_undo();
        let completed_levels =
            core::array::from_fn::<_, LEVEL_COUNT, _>(|index| model.is_completed(index));
        let puzzle = format!("PUZZLE {:02} / 05", level_index + 1);
        let moves = format!("{moves:02}");
        let completed = format!("{completed} / 5");
        let hint_note = match hint {
            Some(0) => "试试镜片 1",
            Some(1) => "试试镜片 2",
            Some(2) => "试试镜片 3",
            Some(3) => "试试镜片 4",
            Some(4) => "试试镜片 5",
            Some(5) => "试试镜片 6",
            Some(6) => "试试镜片 7",
            _ => "镜片只在两种方向间切换",
        };
        let _ = model;

        for (entity, content) in [
            (nodes.level_name, level_name.into()),
            (nodes.level_subtitle, level_subtitle.into()),
            (nodes.puzzle, puzzle),
            (
                nodes.status,
                if solved {
                    "光路接通"
                } else {
                    "等待点亮"
                }
                .into(),
            ),
            (
                nodes.status_subtitle,
                if solved {
                    "NICE CONNECTION"
                } else {
                    "FOLLOW THE LIGHT"
                }
                .into(),
            ),
            (nodes.moves, moves),
            (nodes.completed, completed),
            (nodes.hint_note, hint_note.into()),
            (nodes.scan, if scan { "追光 开" } else { "追光 关" }.into()),
        ] {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }

        set_button_state(world, nodes.undo, can_undo, false);
        set_button_state(world, nodes.scan, true, scan);
        set_button_state(world, nodes.next, true, solved);
        if let Some(style) = world.get_mut::<Style>(nodes.status) {
            style.text_color = if solved { SUCCESS.into() } else { TEXT.into() };
        }
        world.invalidate_visual(nodes.status);

        set_hidden(world, nodes.modal, !levels_open);
        for (index, entity) in nodes.level_checks.into_iter().enumerate() {
            set_hidden(world, entity, !completed_levels[index]);
        }
        for (index, entity) in nodes.mirror_labels.into_iter().enumerate() {
            set_hidden(world, entity, index >= mirror_count);
            if index < mirror_count
                && let Some(style) = world.get_mut::<Style>(entity)
            {
                let position = mirror_positions[index];
                style.layout.left = Dimension::px(31 + i32::from(position.x) * 38);
                style.layout.top = Dimension::px(10 + i32::from(position.y) * 38);
            }
            world.invalidate(entity);
        }
    }
}

fn set_hidden(world: &mut World, entity: Entity, hidden: bool) {
    if hidden {
        if !world.has::<Hidden>(entity) {
            world.insert(entity, Hidden);
        }
    } else {
        world.remove::<Hidden>(entity);
    }
    world.invalidate(entity);
}

fn set_button_state(world: &mut World, entity: Entity, enabled: bool, active: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            ACCENT.into()
        } else if enabled {
            CONTROL.into()
        } else {
            Color::rgb(35, 47, 43).into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BACKGROUND.into()
        } else if enabled {
            TEXT.into()
        } else {
            Color::rgb(93, 108, 101).into()
        };
    }
    world.invalidate_visual(entity);
}

fn cell_center(point: crate::gallery::play::lumen::GridPoint) -> Point {
    Point::new(27 + i32::from(point.x) * 38, 26 + i32::from(point.y) * 38)
}

fn paint_trace(painter: &mut PlayPainter<'_, '_>, trace: &Trace) {
    let points = trace.points();
    for pair in points.windows(2) {
        let from = cell_center(pair[0]);
        let to = cell_center(pair[1]);
        painter.line(from, to, ACCENT, Fixed::from_ratio(12, 5));
        painter.line(from, to, LIGHT, Fixed::ONE);
    }
}

fn scan_point(trace: &Trace, phase: u16) -> Option<Point> {
    let points = trace.points();
    let segments = points.len().checked_sub(1)?;
    let scaled = u32::from(phase) * segments as u32;
    let segment = ((scaled >> 16) as usize).min(segments - 1);
    let fraction = Fixed::from_ratio((scaled & 0xffff) as i32, 65_536);
    let from = cell_center(points[segment]);
    let to = cell_center(points[segment + 1]);
    Some(Point {
        x: from.x + (to.x - from.x) * fraction,
        y: from.y + (to.y - from.y) * fraction,
    })
}

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &LumenModel) {
    painter.fill(Rect::new(0, 0, 282, 204), BOARD, Fixed::from_int(9));
    painter.border(
        Rect::new(0, 0, 282, 204),
        Color::rgb(54, 68, 67),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for row in 0..5 {
        for column in 0..7 {
            let center = Point::new(27 + column * 38, 26 + row * 38);
            painter.fill(
                Rect::new(
                    center.x - Fixed::from_int(17),
                    center.y - Fixed::from_int(17),
                    34,
                    34,
                ),
                TILE,
                Fixed::from_int(5),
            );
            painter.circle(center, Fixed::ONE, Color::rgb(82, 96, 91));
        }
    }

    paint_trace(painter, model.trace());
    if model.scan() {
        for offset in [0_u16, 21_845, 43_690] {
            if let Some(point) = scan_point(model.trace(), model.scan_phase().wrapping_add(offset))
            {
                painter.circle(point, Fixed::from_int(2), LIGHT);
            }
        }
    }

    for wall in model.level().walls {
        let center = cell_center(*wall);
        painter.fill(
            Rect::new(
                center.x - Fixed::from_int(13),
                center.y - Fixed::from_int(13),
                26,
                26,
            ),
            Color::rgb(59, 68, 68),
            Fixed::from_int(4),
        );
        painter.border(
            Rect::new(
                center.x - Fixed::from_int(13),
                center.y - Fixed::from_int(13),
                26,
                26,
            ),
            Color::rgb(89, 97, 92),
            Fixed::ONE,
            Fixed::from_int(4),
        );
        for offset in [-6, 0, 6] {
            painter.line(
                Point::new(
                    center.x - Fixed::from_int(8),
                    center.y + Fixed::from_int(offset),
                ),
                Point::new(
                    center.x + Fixed::from_int(8),
                    center.y + Fixed::from_int(offset),
                ),
                Color::rgb(98, 107, 96),
                Fixed::ONE,
            );
        }
    }

    for (index, mirror) in model.level().mirrors.iter().enumerate() {
        let center = cell_center(mirror.position);
        let selected = model.selected() == index;
        let hinted = model.hint() == Some(index);
        painter.fill(
            Rect::new(
                center.x - Fixed::from_int(15),
                center.y - Fixed::from_int(15),
                30,
                30,
            ),
            if selected {
                Color::rgb(56, 68, 71)
            } else {
                Color::rgb(44, 57, 61)
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(
                center.x - Fixed::from_int(15),
                center.y - Fixed::from_int(15),
                30,
                30,
            ),
            if hinted {
                Color::rgb(237, 178, 151)
            } else if selected {
                ACCENT
            } else {
                Color::rgb(115, 134, 133)
            },
            if hinted {
                Fixed::from_int(2)
            } else {
                Fixed::ONE
            },
            Fixed::from_int(6),
        );
        let slash = model.orientations()[index] == MirrorOrientation::Slash;
        let first = Point::new(
            center.x - Fixed::from_int(9),
            center.y + Fixed::from_int(if slash { 9 } else { -9 }),
        );
        let second = Point::new(
            center.x + Fixed::from_int(9),
            center.y + Fixed::from_int(if slash { -9 } else { 9 }),
        );
        painter.line(first, second, Color::rgb(220, 234, 221), Fixed::from_int(2));
        painter.circle(first, Fixed::from_int(2), ACCENT);
        painter.circle(second, Fixed::from_int(2), ACCENT);
    }

    let target = cell_center(model.level().target);
    painter.circle(target, Fixed::from_int(12), Color::rgb(161, 195, 176));
    painter.circle(target, Fixed::from_int(10), Color::rgb(26, 41, 39));
    painter.circle(
        target,
        Fixed::from_int(6),
        if model.trace().solved {
            SUCCESS
        } else {
            Color::rgb(26, 41, 39)
        },
    );
    if model.trace().solved {
        painter.line(
            Point::new(target.x - Fixed::from_int(4), target.y),
            Point::new(target.x - Fixed::ONE, target.y + Fixed::from_int(3)),
            BACKGROUND,
            Fixed::from_int(2),
        );
        painter.line(
            Point::new(target.x - Fixed::ONE, target.y + Fixed::from_int(3)),
            Point::new(target.x + Fixed::from_int(5), target.y - Fixed::from_int(4)),
            BACKGROUND,
            Fixed::from_int(2),
        );
    }

    let source_y = cell_center(crate::gallery::play::lumen::GridPoint {
        x: 0,
        y: model.level().source_row,
    })
    .y;
    painter.line(
        Point::new(2, source_y - Fixed::from_int(5)),
        Point::new(8, source_y),
        ACCENT,
        Fixed::from_int(2),
    );
    painter.line(
        Point::new(8, source_y),
        Point::new(2, source_y + Fixed::from_int(5)),
        ACCENT,
        Fixed::from_int(2),
    );
}

fn board_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<LumenModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 282, 204);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_board(&mut painter, model);
}

fn board_view() -> View {
    View::new("LumenBoard", 60, board_render).with_filter::<LumenBoard>()
}

fn board_tap(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
        return false;
    };
    if rect.w.is_zero() || rect.h.is_zero() {
        return false;
    }
    let local_x = (*x - rect.x) * Fixed::from_int(282) / rect.w;
    let local_y = (*y - rect.y) * Fixed::from_int(204) / rect.h;
    let column = ((local_x.to_int() - 8) / 38) as i8;
    let row = ((local_y.to_int() - 7) / 38) as i8;
    if !(0..7).contains(&column) || !(0..5).contains(&row) {
        return false;
    }
    LumenNodes::update(world, |model| model.rotate_cell(column, row));
    true
}

#[mirui_macros::system(order = ANIMATION)]
fn lumen_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    LumenNodes::update(world, |model| model.advance_ms(elapsed));
}

fn padding(vertical: i32, horizontal: i32) -> Padding {
    Padding {
        top: Dimension::px(vertical),
        right: Dimension::px(horizontal),
        bottom: Dimension::px(vertical),
        left: Dimension::px(horizontal),
    }
}

#[compose]
fn build_widgets() {
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

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    register_play_font(&mut app.world);
    app.with_widget(board_view());
    app.world.insert_resource(LumenModel::new());
    app.add_system(lumen_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| {
        app.world
            .find_by_id(id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };
    app.world.insert_resource(LumenNodes {
        board: find("lumen_board"),
        level_name: find("lumen_level_name"),
        level_subtitle: find("lumen_level_subtitle"),
        puzzle: find("lumen_puzzle"),
        status: find("lumen_status"),
        status_subtitle: find("lumen_status_subtitle"),
        moves: find("lumen_moves"),
        completed: find("lumen_completed"),
        hint_note: find("lumen_hint_note"),
        undo: find("lumen_undo"),
        scan: find("lumen_scan"),
        next: find("lumen_next"),
        modal: find("lumen_levels_modal"),
        mirror_labels: [
            find("lumen_mirror_1"),
            find("lumen_mirror_2"),
            find("lumen_mirror_3"),
            find("lumen_mirror_4"),
            find("lumen_mirror_5"),
            find("lumen_mirror_6"),
            find("lumen_mirror_7"),
        ],
        level_checks: [
            find("lumen_level_check_1"),
            find("lumen_level_check_2"),
            find("lumen_level_check_3"),
            find("lumen_level_check_4"),
            find("lumen_level_check_5"),
        ],
    });
    LumenNodes::sync(&mut app.world);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::view::ViewRegistry;
    use crate::ui::{IdMap, UiScope};

    fn fixture() -> World {
        let mut world = World::new();
        let mut registry = ViewRegistry::with_builtins();
        registry.insert(board_view());
        world.insert_resource(registry);
        world.insert_resource(IdMap::new());
        world.insert_resource(LumenModel::new());
        let root = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, root);
        build_widgets(&mut cx);
        let find = |id| world.find_by_id(id).unwrap();
        world.insert_resource(LumenNodes {
            board: find("lumen_board"),
            level_name: find("lumen_level_name"),
            level_subtitle: find("lumen_level_subtitle"),
            puzzle: find("lumen_puzzle"),
            status: find("lumen_status"),
            status_subtitle: find("lumen_status_subtitle"),
            moves: find("lumen_moves"),
            completed: find("lumen_completed"),
            hint_note: find("lumen_hint_note"),
            undo: find("lumen_undo"),
            scan: find("lumen_scan"),
            next: find("lumen_next"),
            modal: find("lumen_levels_modal"),
            mirror_labels: core::array::from_fn(|index| {
                world
                    .find_by_id(match index {
                        0 => "lumen_mirror_1",
                        1 => "lumen_mirror_2",
                        2 => "lumen_mirror_3",
                        3 => "lumen_mirror_4",
                        4 => "lumen_mirror_5",
                        5 => "lumen_mirror_6",
                        _ => "lumen_mirror_7",
                    })
                    .unwrap()
            }),
            level_checks: core::array::from_fn(|index| {
                world
                    .find_by_id(match index {
                        0 => "lumen_level_check_1",
                        1 => "lumen_level_check_2",
                        2 => "lumen_level_check_3",
                        3 => "lumen_level_check_4",
                        _ => "lumen_level_check_5",
                    })
                    .unwrap()
            }),
        });
        LumenNodes::sync(&mut world);
        world
    }

    #[test]
    fn composition_uses_real_controls_and_one_dense_board() {
        let world = fixture();
        assert!(world.query::<Text>().collect().len() >= 20);
        assert!(world.query::<Button>().collect().len() >= 10);
        assert_eq!(world.query::<LumenBoard>().collect().len(), 1);
    }

    #[test]
    fn grid_coordinates_promote_before_pixel_scaling() {
        assert_eq!(
            cell_center(crate::gallery::play::lumen::GridPoint { x: 6, y: 4 }),
            Point::new(255, 178)
        );
    }

    #[test]
    fn tapping_a_mirror_uses_board_local_coordinates() {
        let mut world = fixture();
        let board = world.find_by_id("lumen_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(40, 80, 564, 408)));
        assert!(board_tap(
            &mut world,
            board,
            &GestureEvent::Tap {
                x: Fixed::from_int(40 + (27 + 2 * 38) * 2),
                y: Fixed::from_int(80 + (26 + 3 * 38) * 2),
                target: board,
            }
        ));
        let model = world.resource::<LumenModel>().unwrap();
        assert_eq!(model.moves(), 1);
        assert!(model.trace().solved);
    }

    #[test]
    fn modal_blocks_board_taps_until_a_level_is_selected() {
        let mut world = fixture();
        let board = world.find_by_id("lumen_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 0, 282, 204)));
        LumenNodes::update(&mut world, LumenModel::open_levels);
        let before = world.resource::<LumenModel>().unwrap().moves();
        assert!(board_tap(
            &mut world,
            board,
            &GestureEvent::Tap {
                x: Fixed::from_int(2 * 38 + 27),
                y: Fixed::from_int(3 * 38 + 26),
                target: board,
            }
        ));
        assert_eq!(world.resource::<LumenModel>().unwrap().moves(), before);
    }
}
