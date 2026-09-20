extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::moss::{
    GRID_HEIGHT, GRID_WIDTH, MossCells, MossModal, MossModel, MossTool,
};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BACKGROUND: Color = Color::rgb(31, 42, 33);
const HEADER: Color = Color::rgb(34, 47, 37);
const BOARD: Color = Color::rgb(17, 29, 23);
const PANEL: Color = Color::rgb(38, 56, 42);
const CONTROL: Color = Color::rgb(49, 67, 50);
const ACTIVE: Color = Color::rgb(208, 236, 157);
const TEXT: Color = Color::rgb(237, 242, 219);
const MUTED: Color = Color::rgb(156, 172, 138);

#[derive(crate::Component, Default)]
struct MossSurface;

#[derive(crate::Component, Default)]
struct MossModalSurface;

#[derive(Clone, Copy)]
struct MossNodes {
    surface: Entity,
    status: Entity,
    generation: Entity,
    live: Entity,
    rate: Entity,
    undo: Entity,
    run: Entity,
    rotate: Entity,
    tools: [Entity; 3],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    seed_buttons: [Entity; 3],
    clear_controls: [Entity; 4],
}

impl MossNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut MossModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<MossModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::VISUAL)
            && let Some(surface) = world.resource::<Self>().map(|nodes| nodes.surface)
        {
            world.invalidate_visual(surface);
        }
        if changes.contains(ChangeSet::MODEL) || changes.contains(ChangeSet::LAYOUT) {
            Self::sync(world);
        }
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<MossModel>() else {
            return;
        };
        let running = model.running();
        let tool = model.tool();
        let rotation = model.rotation();
        let history_len = model.history_len();
        let modal = model.modal();
        let seed_id = model.seed_id();
        let live = model.live_count();
        let texts = [
            (
                nodes.status,
                if running {
                    "B3 / S23 · RUN".into()
                } else {
                    "B3 / S23 · PAUSE".into()
                },
            ),
            (nodes.generation, format!("{:03}", model.generation())),
            (nodes.live, format!("{live}")),
            (nodes.rate, format!("{} 代/秒 ↻", model.rate())),
            (
                nodes.run,
                if running {
                    "暂停".into()
                } else {
                    "运行".into()
                },
            ),
            (nodes.rotate, format!("{} 度旋转", rotation * 90)),
        ];
        for (entity, content) in texts {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for (index, entity) in nodes.tools.into_iter().enumerate() {
            let active = matches!(
                (index, tool),
                (0, MossTool::Plant) | (1, MossTool::Erase) | (2, MossTool::Glider)
            );
            set_button_state(world, entity, active, true);
        }
        set_button_state(world, nodes.rotate, false, tool == MossTool::Glider);
        set_button_state(world, nodes.run, running, modal == MossModal::None);
        set_button_state(
            world,
            nodes.undo,
            false,
            history_len > 0 && modal == MossModal::None,
        );
        set_hidden(world, nodes.modal, modal == MossModal::None);
        let seeds_open = modal == MossModal::Seeds;
        let clear_open = modal == MossModal::Clear;
        for (index, entity) in nodes.seed_buttons.into_iter().enumerate() {
            set_hidden(world, entity, !seeds_open);
            set_button_state(world, entity, seed_id as usize == index, seeds_open);
        }
        for entity in nodes.clear_controls {
            set_hidden(world, entity, !clear_open);
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_title) {
            text.set_content(if seeds_open {
                "给花园一种新的开始"
            } else {
                "让花园重新开始？"
            });
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_subtitle) {
            text.set_content(if seeds_open {
                "载入会暂停演化；新种子可以撤销。".into()
            } else {
                format!("当前有 {live} 个活细胞；清空后代数归零。")
            });
        }
        world.invalidate(nodes.modal_title);
        world.invalidate(nodes.modal_subtitle);
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

fn set_button_state(world: &mut World, entity: Entity, active: bool, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            ACTIVE.into()
        } else if enabled {
            CONTROL.into()
        } else {
            Color::rgb(40, 51, 41).into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BACKGROUND.into()
        } else if enabled {
            TEXT.into()
        } else {
            Color::rgb(105, 119, 100).into()
        };
    }
    world.invalidate_visual(entity);
}

fn paint_cells(painter: &mut PlayPainter<'_, '_>, cells: &MossCells, previous: &MossCells) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let age = cells.get(x, y);
            let old = previous.get(x, y);
            let left = 17 + i32::from(x) * 17;
            let top = 67 + i32::from(y) * 17;
            let fill = if age == 0 {
                Color::rgb(27, 44, 33)
            } else if age == 1 {
                Color::rgb(208, 236, 157)
            } else if age < 5 {
                Color::rgb(157, 207, 132)
            } else {
                Color::rgb(120, 171, 112)
            };
            painter.fill(Rect::new(left, top, 15, 15), fill, Fixed::from_int(3));
            if age != 0 {
                painter.line(
                    Point::new(left + 4, top + 11),
                    Point::new(left + 10, top + 5),
                    if age == 1 {
                        Color::rgb(138, 184, 101)
                    } else {
                        Color::rgb(84, 137, 77)
                    },
                    Fixed::ONE,
                );
                painter.circle(
                    Point::new(left + 5, top + 4),
                    Fixed::ONE,
                    if age == 1 {
                        Color::rgb(237, 245, 205)
                    } else {
                        Color::rgb(182, 220, 150)
                    },
                );
            } else if old != 0 {
                painter.fill(
                    Rect::new(left + 5, top + 5, 5, 5),
                    Color::rgb(67, 88, 65),
                    Fixed::ONE,
                );
            } else {
                painter.fill(
                    Rect::new(left + 7, top + 7, 1, 1),
                    Color::rgb(64, 80, 62),
                    Fixed::ZERO,
                );
            }
        }
    }
}

fn paint_moss_surface(painter: &mut PlayPainter<'_, '_>, model: &MossModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(57, 73, 58),
        Fixed::ONE,
    );
    painter.line(
        Point::new(18, 22),
        Point::new(24, 12),
        ACTIVE,
        Fixed::from_int(2),
    );
    painter.circle(Point::new(17, 14), Fixed::from_int(3), ACTIVE);
    painter.circle(Point::new(23, 10), Fixed::from_int(2), ACTIVE);
    painter.fill(Rect::new(12, 64, 350, 211), BOARD, Fixed::from_int(7));
    painter.border(
        Rect::new(12, 64, 350, 211),
        Color::rgb(60, 81, 60),
        Fixed::ONE,
        Fixed::from_int(7),
    );
    paint_cells(painter, model.cells(), model.previous());
    painter.fill(Rect::new(371, 42, 97, 116), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(371, 42, 97, 116),
        Color::rgb(67, 86, 64),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    painter.line(
        Point::new(382, 100),
        Point::new(457, 100),
        Color::rgb(64, 88, 60),
        Fixed::ONE,
    );
    painter.fill(
        Rect::new(382, 136, 74, 4),
        Color::rgb(58, 80, 53),
        Fixed::from_int(2),
    );
    let density = Fixed::from_int(74) * Fixed::from_ratio(i32::from(model.live_count()), 240);
    painter.fill(
        Rect {
            x: Fixed::from_int(382),
            y: Fixed::from_int(136),
            w: density,
            h: Fixed::from_int(4),
        },
        ACTIVE,
        Fixed::from_int(2),
    );
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(24, 36, 27),
        Fixed::ZERO,
    );
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<MossModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_moss_surface(&mut painter, model);
}

fn paint_seed_preview(
    painter: &mut PlayPainter<'_, '_>,
    cells: &MossCells,
    origin_x: i32,
    origin_y: i32,
) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            if cells.get(x, y) != 0 {
                painter.fill(
                    Rect::new(
                        origin_x + i32::from(x) * 6,
                        origin_y + i32::from(y) * 6,
                        5,
                        5,
                    ),
                    ACTIVE,
                    Fixed::ONE,
                );
            }
        }
    }
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<MossModel>() else {
        return;
    };
    if model.modal() == MossModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(8, 13, 9, 224),
        Fixed::ZERO,
    );
    painter.fill(
        Rect::new(17, 65, 446, 206),
        Color::rgb(32, 50, 37),
        Fixed::from_int(10),
    );
    painter.border(
        Rect::new(17, 65, 446, 206),
        Color::rgb(72, 96, 67),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == MossModal::Seeds {
        for seed_id in 0..3_u8 {
            let left = 29 + i32::from(seed_id) * 142;
            painter.fill(
                Rect::new(left, 104, 135, 111),
                Color::rgb(32, 50, 37),
                Fixed::from_int(7),
            );
            painter.border(
                Rect::new(left, 104, 135, 111),
                if model.seed_id() == seed_id {
                    ACTIVE
                } else {
                    Color::rgb(72, 96, 67)
                },
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let cells = MossCells::seed(seed_id).expect("built-in seed");
            paint_seed_preview(&mut painter, &cells, left + 8, 122);
        }
    } else {
        painter.fill(
            Rect::new(57, 116, 84, 84),
            Color::rgb(23, 37, 28),
            Fixed::from_int(42),
        );
        painter.line(
            Point::new(83, 177),
            Point::new(111, 135),
            Color::rgb(64, 91, 59),
            Fixed::from_int(4),
        );
        painter.circle(Point::new(83, 143), Fixed::from_int(12), ACTIVE);
        painter.circle(Point::new(111, 133), Fixed::from_int(9), ACTIVE);
    }
}

fn surface_view() -> View {
    View::new("MossSurface", 60, surface_render).with_filter::<MossSurface>()
}

fn modal_view() -> View {
    View::new("MossModalSurface", 70, modal_render).with_filter::<MossModalSurface>()
}

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    if local_x < Fixed::from_int(17)
        || local_x >= Fixed::from_int(357)
        || local_y < Fixed::from_int(67)
        || local_y >= Fixed::from_int(271)
    {
        return None;
    }
    Some((
        ((local_x.to_int() - 17) / 17) as u8,
        ((local_y.to_int() - 67) / 17) as u8,
    ))
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            MossNodes::update(world, |model| {
                model.begin_stroke(cell_x, cell_y) | model.end_stroke(false)
            });
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            MossNodes::update(world, |model| model.begin_stroke(cell_x, cell_y));
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return true;
            };
            MossNodes::update(world, |model| model.continue_stroke(cell_x, cell_y));
        }
        GestureEvent::DragEnd { .. } => {
            MossNodes::update(world, |model| model.end_stroke(false));
        }
        GestureEvent::DragCancel { .. } => {
            MossNodes::update(world, |model| model.end_stroke(true));
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = ANIMATION)]
fn moss_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    MossNodes::update(world, |model| model.advance_ms(elapsed));
}

#[compose]
fn build_widgets() {
    ui! {
        MossSurface (id: "moss_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "MOSS STUDY",
                position: Position::Absolute,
                left: 35,
                top: 7,
                width: 220,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "B3 / S23 · PAUSE",
                id: "moss_status",
                position: Position::Absolute,
                left: 300,
                top: 8,
                width: 160,
                height: 20,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "播种",
                id: "moss_tool_plant",
                position: Position::Absolute,
                left: 13,
                top: 41,
                width: 64,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Plant)); }
            Button (
                "擦除",
                id: "moss_tool_erase",
                position: Position::Absolute,
                left: 82,
                top: 41,
                width: 64,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Erase)); }
            Button (
                "滑翔机",
                id: "moss_tool_glider",
                position: Position::Absolute,
                left: 151,
                top: 41,
                width: 79,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Glider)); }
            Button (
                "0 度旋转",
                id: "moss_rotate",
                position: Position::Absolute,
                left: 235,
                top: 41,
                width: 119,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: MUTED,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, MossModel::rotate_glider); }
            Text (
                "GENERATION",
                position: Position::Absolute,
                left: 382,
                top: 52,
                width: 74,
                height: 13,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "000",
                id: "moss_generation",
                position: Position::Absolute,
                left: 382,
                top: 64,
                width: 74,
                height: 30,
                font_size: 24,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "活细胞",
                position: Position::Absolute,
                left: 382,
                top: 105,
                width: 45,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "moss_live",
                position: Position::Absolute,
                left: 425,
                top: 102,
                width: 31,
                height: 22,
                font_size: 17,
                text_color: Color::rgb(213, 233, 192),
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "4 代/秒 ↻",
                id: "moss_rate",
                position: Position::Absolute,
                left: 372,
                top: 168,
                width: 96,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::cycle_rate); }
            Button (
                "撤销",
                id: "moss_undo",
                position: Position::Absolute,
                left: 372,
                top: 204,
                width: 96,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::undo); }
            Text (
                "边缘之外为空",
                position: Position::Absolute,
                left: 371,
                top: 242,
                width: 97,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "生命 ≠ 生物模拟",
                position: Position::Absolute,
                left: 371,
                top: 257,
                width: 97,
                height: 13,
                font_size: 7,
                text_color: Color::rgb(128, 153, 115),
                paragraph: ParagraphStyle::label()
            )
            Button (
                "运行",
                id: "moss_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::toggle_running); }
            Button (
                "单步",
                position: Position::Absolute,
                left: 127,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::step); }
            Button (
                "种子",
                position: Position::Absolute,
                left: 242,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::open_seeds); }
            Button (
                "清空",
                position: Position::Absolute,
                left: 357,
                top: 288,
                width: 111,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::open_clear); }
            MossModalSurface (
                id: "moss_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    "给花园一种新的开始",
                    id: "moss_modal_title",
                    position: Position::Absolute,
                    left: 29,
                    top: 75,
                    width: 390,
                    height: 22,
                    font_size: 13,
                    text_color: ACTIVE,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "载入会暂停演化；新种子可以撤销。",
                    id: "moss_modal_subtitle",
                    position: Position::Absolute,
                    left: 29,
                    top: 94,
                    width: 410,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 428,
                    top: 73,
                    width: 24,
                    height: 22,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MossNodes::update(ctx.world, MossModel::close_modal); }
                Button (
                    "漂流花园",
                    id: "moss_seed_0",
                    position: Position::Absolute,
                    left: 29,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(0)); }
                Button (
                    "双生脉冲",
                    id: "moss_seed_1",
                    position: Position::Absolute,
                    left: 171,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(1)); }
                Button (
                    "固定随机种子",
                    id: "moss_seed_2",
                    position: Position::Absolute,
                    left: 313,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(2)); }
                Text (
                    "清空后可以重新播种，也可以撤销。",
                    id: "moss_clear_note",
                    position: Position::Absolute,
                    left: 171,
                    top: 139,
                    width: 257,
                    height: 18,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "清空花园",
                    id: "moss_clear_confirm",
                    position: Position::Absolute,
                    left: 171,
                    top: 190,
                    width: 257,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACTIVE,
                    pressed_color: ACTIVE,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, MossModel::confirm_clear); }
                Button (
                    "保留花园",
                    id: "moss_clear_cancel",
                    position: Position::Absolute,
                    left: 171,
                    top: 230,
                    width: 257,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, MossModel::close_modal); }
                Text (
                    "可撤销",
                    id: "moss_clear_badge",
                    position: Position::Absolute,
                    left: 72,
                    top: 209,
                    width: 54,
                    height: 16,
                    font_size: 8,
                    text_color: ACTIVE,
                    paragraph: ParagraphStyle::label()
                )
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
    app.world.insert_resource(MossModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(moss_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Moss Study node");
    let nodes = MossNodes {
        surface: find("moss_surface"),
        status: find("moss_status"),
        generation: find("moss_generation"),
        live: find("moss_live"),
        rate: find("moss_rate"),
        undo: find("moss_undo"),
        run: find("moss_run"),
        rotate: find("moss_rotate"),
        tools: [
            find("moss_tool_plant"),
            find("moss_tool_erase"),
            find("moss_tool_glider"),
        ],
        modal: find("moss_modal"),
        modal_title: find("moss_modal_title"),
        modal_subtitle: find("moss_modal_subtitle"),
        seed_buttons: [
            find("moss_seed_0"),
            find("moss_seed_1"),
            find("moss_seed_2"),
        ],
        clear_controls: [
            find("moss_clear_note"),
            find("moss_clear_confirm"),
            find("moss_clear_cancel"),
            find("moss_clear_badge"),
        ],
    };
    app.world.insert_resource(nodes);
    MossNodes::sync(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_uses_one_dense_surface_and_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("moss_surface").is_some());
        assert!(app.world.find_by_id("moss_run").is_some());
        assert!(app.world.find_by_id("moss_seed_2").is_some());
        assert_eq!(app.world.query::<MossSurface>().iter().count(), 1);
    }

    #[test]
    fn cancelled_drag_restores_the_whole_garden_transaction() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        let surface = app.world.find_by_id("moss_surface").unwrap();
        assert_eq!(
            local_cell(
                &app.world,
                surface,
                Fixed::from_int(16),
                Fixed::from_int(67)
            ),
            None
        );
        let before = *app.world.resource::<MossModel>().unwrap().cells();
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragStart {
                x: Fixed::from_int(18),
                y: Fixed::from_int(68),
                target: surface,
            },
        );
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragMove {
                x: Fixed::from_int(350),
                y: Fixed::from_int(265),
                dx: Fixed::from_int(332),
                dy: Fixed::from_int(197),
                target: surface,
            },
        );
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragCancel {
                x: Fixed::from_int(350),
                y: Fixed::from_int(265),
                target: surface,
            },
        );
        assert_eq!(*app.world.resource::<MossModel>().unwrap().cells(), before);
    }
}
