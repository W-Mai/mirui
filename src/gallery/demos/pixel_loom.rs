extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::pixel::{
    FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelFrames, PixelModal, PixelModel, PixelTool,
};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BACKGROUND: Color = Color::rgb(36, 36, 46);
const HEADER: Color = Color::rgb(38, 38, 49);
const BOARD: Color = Color::rgb(18, 26, 32);
const PANEL: Color = Color::rgb(40, 41, 54);
const CONTROL: Color = Color::rgb(45, 60, 53);
const ACTIVE: Color = Color::rgb(203, 179, 240);
const TEXT: Color = Color::rgb(233, 237, 225);
const MUTED: Color = Color::rgb(160, 170, 156);
const PALETTE: [Color; 7] = [
    Color::rgb(29, 34, 43),
    Color::rgb(203, 179, 240),
    Color::rgb(185, 229, 171),
    Color::rgb(240, 211, 132),
    Color::rgb(234, 165, 134),
    Color::rgb(167, 212, 236),
    Color::rgb(242, 238, 224),
];
const TEMPLATE_NAMES: [&str; 3] = ["星际来客", "风中绿芽", "纸上飞行"];

#[derive(crate::Component, Default)]
struct PixelSurface;

#[derive(crate::Component, Default)]
struct PixelModalSurface;

#[derive(Clone, Copy)]
struct PixelNodes {
    surface: Entity,
    frame_status: Entity,
    mode_status: Entity,
    template_name: Entity,
    fps: Entity,
    play: Entity,
    undo: Entity,
    frames: [Entity; 4],
    tools: [Entity; 4],
    colors: [Entity; 6],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    template_buttons: [Entity; 3],
    clear_controls: [Entity; 3],
}

impl PixelNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut PixelModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<PixelModel>()
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
        let Some(model) = world.resource::<PixelModel>() else {
            return;
        };
        let frame = model.frame();
        let visible_frame = model.visible_frame();
        let playing = model.playing();
        let template_id = model.template_id();
        let fps = model.fps();
        let tool = model.tool();
        let mirror = model.mirror();
        let onion = model.onion();
        let color = model.color();
        let history_len = model.history_len();
        let modal = model.modal();
        let texts = [
            (
                nodes.frame_status,
                if playing {
                    format!("PLAY · {fps} FPS")
                } else {
                    format!("FRAME {} / 4", frame + 1)
                },
            ),
            (
                nodes.mode_status,
                if playing {
                    "正在播放".into()
                } else {
                    "画一格，就改变一点".into()
                },
            ),
            (
                nodes.template_name,
                TEMPLATE_NAMES[template_id as usize].into(),
            ),
            (nodes.fps, format!("{fps} 帧/秒 ↻")),
            (
                nodes.play,
                if playing {
                    "暂停预览".into()
                } else {
                    "播放动画".into()
                },
            ),
        ];
        for (entity, content) in texts {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for (index, entity) in nodes.frames.into_iter().enumerate() {
            set_frame_state(world, entity, visible_frame as usize == index, !playing);
        }
        for (index, entity) in nodes.tools.into_iter().enumerate() {
            let active = match index {
                0 => tool == PixelTool::Brush,
                1 => tool == PixelTool::Erase,
                2 => mirror,
                _ => onion,
            };
            set_button_state(world, entity, active, !playing);
        }
        for (index, entity) in nodes.colors.into_iter().enumerate() {
            set_palette_state(
                world,
                entity,
                color as usize == index + 1,
                !playing,
                index + 1,
            );
        }
        set_button_state(world, nodes.play, playing, true);
        set_button_state(world, nodes.undo, false, history_len > 0 && !playing);
        set_hidden(world, nodes.modal, modal == PixelModal::None);
        let template_open = modal == PixelModal::Templates;
        let clear_open = modal == PixelModal::Clear;
        for entity in nodes.template_buttons {
            set_hidden(world, entity, !template_open);
        }
        for entity in nodes.clear_controls {
            set_hidden(world, entity, !clear_open);
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_title) {
            text.set_content(if template_open {
                "先借一颗灵感"
            } else {
                "清空这一帧？"
            });
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_subtitle) {
            text.set_content(if template_open {
                "载入模板会替换四帧；可以撤销，不会写入存储。"
            } else {
                "其他三帧不受影响；清空以后也能撤销。"
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
            Color::rgb(42, 45, 51).into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BACKGROUND.into()
        } else if enabled {
            TEXT.into()
        } else {
            Color::rgb(105, 112, 107).into()
        };
    }
    world.invalidate_visual(entity);
}

fn set_frame_state(world: &mut World, entity: Entity, active: bool, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = Color::rgba(0, 0, 0, 0).into();
        button.pressed_color = Color::rgba(0, 0, 0, 0).into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            ACTIVE.into()
        } else if enabled {
            MUTED.into()
        } else {
            Color::rgb(105, 112, 107).into()
        };
    }
    world.invalidate_visual(entity);
}

fn set_palette_state(world: &mut World, entity: Entity, active: bool, enabled: bool, index: usize) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if enabled {
            PALETTE[index].into()
        } else {
            Color::rgb(61, 62, 64).into()
        };
        button.pressed_color = PALETTE[index].into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.border_color = Some(if active {
            Color::rgb(255, 247, 220).into()
        } else {
            Color::rgba(0, 0, 0, 0).into()
        });
        style.border_width = if active {
            Fixed::from_int(2)
        } else {
            Fixed::ZERO
        };
    }
    world.invalidate_visual(entity);
}

fn paint_sprite(
    painter: &mut PlayPainter<'_, '_>,
    frames: &PixelFrames,
    frame: u8,
    origin: Point,
    cell: Fixed,
    checker: bool,
) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let color = frames.get(frame, x, y);
            if color == 0 && !checker {
                continue;
            }
            let fill = if color == 0 {
                if (x + y) & 1 == 0 {
                    Color::rgb(32, 37, 44)
                } else {
                    Color::rgb(37, 41, 50)
                }
            } else {
                PALETTE[color as usize]
            };
            painter.fill(
                Rect {
                    x: origin.x + Fixed::from_int(i32::from(x)) * cell,
                    y: origin.y + Fixed::from_int(i32::from(y)) * cell,
                    w: cell,
                    h: cell,
                },
                fill,
                Fixed::ZERO,
            );
        }
    }
}

fn paint_pixel_surface(painter: &mut PlayPainter<'_, '_>, model: &PixelModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(55, 58, 65),
        Fixed::ONE,
    );
    for (x, y) in [(15, 12), (22, 12), (15, 19), (22, 19)] {
        painter.fill(Rect::new(x, y, 4, 4), ACTIVE, Fixed::ONE);
    }
    painter.fill(Rect::new(12, 54, 202, 202), BOARD, Fixed::from_int(6));
    painter.border(
        Rect::new(12, 54, 202, 202),
        Color::rgb(76, 70, 95),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    let frame = model.visible_frame();
    let previous = (model.frame() + FRAME_COUNT - 1) % FRAME_COUNT;
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let color = model.frames().get(frame, x, y);
            let ghost = !model.playing()
                && model.onion()
                && color == 0
                && model.frames().get(previous, x, y) != 0;
            let fill = if color != 0 {
                PALETTE[color as usize]
            } else if ghost {
                Color::rgb(76, 66, 92)
            } else if (x + y) & 1 == 0 {
                Color::rgb(35, 40, 51)
            } else {
                Color::rgb(40, 44, 53)
            };
            painter.fill(
                Rect::new(17 + i32::from(x) * 16, 59 + i32::from(y) * 16, 15, 15),
                fill,
                Fixed::ONE,
            );
        }
    }
    painter.fill(Rect::new(229, 45, 239, 104), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(229, 45, 239, 104),
        Color::rgb(69, 67, 83),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    paint_sprite(
        painter,
        model.frames(),
        frame,
        Point::new(245, 71),
        Fixed::from_ratio(11, 2),
        true,
    );
    for index in 0..4_u8 {
        let x = 230 + i32::from(index) * 60;
        painter.fill(
            Rect::new(x, 162, 55, 50),
            if frame == index {
                Color::rgb(67, 55, 79)
            } else {
                Color::rgb(41, 45, 54)
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(x, 162, 55, 50),
            if frame == index {
                ACTIVE
            } else {
                Color::rgb(72, 80, 90)
            },
            Fixed::ONE,
            Fixed::from_int(6),
        );
        paint_sprite(
            painter,
            model.frames(),
            index,
            Point::new(x + 13, 166),
            Fixed::from_ratio(5, 2),
            false,
        );
    }
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(24, 34, 31),
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
    let Some(model) = world.resource::<PixelModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_pixel_surface(&mut painter, model);
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<PixelModel>() else {
        return;
    };
    if model.modal() == PixelModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(8, 9, 14, 220),
        Fixed::ZERO,
    );
    painter.fill(
        Rect::new(17, 65, 446, 206),
        Color::rgb(39, 40, 51),
        Fixed::from_int(10),
    );
    painter.border(
        Rect::new(17, 65, 446, 206),
        ACTIVE,
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == PixelModal::Templates {
        for template in 0..3_u8 {
            let x = 29 + i32::from(template) * 142;
            painter.fill(Rect::new(x, 104, 135, 111), PANEL, Fixed::from_int(7));
            painter.border(
                Rect::new(x, 104, 135, 111),
                Color::rgb(89, 80, 104),
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let frames = PixelFrames::template(template).expect("built-in template");
            paint_sprite(
                &mut painter,
                &frames,
                0,
                Point::new(x + 32, 113),
                Fixed::from_int(6),
                false,
            );
        }
    } else {
        painter.fill(Rect::new(57, 110, 96, 96), BOARD, Fixed::from_int(5));
        paint_sprite(
            &mut painter,
            model.frames(),
            model.frame(),
            Point::new(57, 110),
            Fixed::from_int(8),
            true,
        );
    }
}

fn surface_view() -> View {
    View::new("PixelSurface", 60, surface_render).with_filter::<PixelSurface>()
}

fn modal_view() -> View {
    View::new("PixelModalSurface", 70, modal_render).with_filter::<PixelModalSurface>()
}

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    if local_x < Fixed::from_int(17)
        || local_x >= Fixed::from_int(209)
        || local_y < Fixed::from_int(59)
        || local_y >= Fixed::from_int(251)
    {
        return None;
    }
    let column = (local_x.to_int() - 17) / 16;
    let row = (local_y.to_int() - 59) / 16;
    if (0..12).contains(&column) && (0..12).contains(&row) {
        Some((column as u8, row as u8))
    } else {
        None
    }
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PixelNodes::update(world, |model| {
                model.begin_stroke(cell_x, cell_y) | model.end_stroke(false)
            });
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PixelNodes::update(world, |model| model.begin_stroke(cell_x, cell_y));
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return true;
            };
            PixelNodes::update(world, |model| model.continue_stroke(cell_x, cell_y));
        }
        GestureEvent::DragEnd { .. } => {
            PixelNodes::update(world, |model| model.end_stroke(false));
        }
        GestureEvent::DragCancel { .. } => {
            PixelNodes::update(world, |model| model.end_stroke(true));
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = ANIMATION)]
fn pixel_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    PixelNodes::update(world, |model| model.advance_ms(elapsed));
}

#[compose]
fn build_widgets() {
    ui! {
        PixelSurface (
            id: "pixel_surface",
            width: 480,
            height: 320,
            clip_children: true
        ) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "PIXEL LOOM",
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
                "FRAME 1 / 4",
                id: "pixel_frame_status",
                position: Position::Absolute,
                left: 350,
                top: 7,
                width: 110,
                height: 22,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "画一格，就改变一点",
                id: "pixel_mode_status",
                position: Position::Absolute,
                left: 17,
                top: 36,
                width: 200,
                height: 17,
                font_size: 10,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "12 × 12",
                position: Position::Absolute,
                left: 150,
                top: 37,
                width: 58,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "LIVE PREVIEW",
                position: Position::Absolute,
                left: 243,
                top: 55,
                width: 120,
                height: 14,
                font_size: 8,
                text_color: Color::rgb(158, 153, 173),
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "星际来客",
                id: "pixel_template_name",
                position: Position::Absolute,
                left: 327,
                top: 72,
                width: 127,
                height: 21,
                font_size: 12,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "四帧小小剧场",
                position: Position::Absolute,
                left: 327,
                top: 94,
                width: 127,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "4 帧/秒 ↻",
                id: "pixel_fps",
                position: Position::Absolute,
                left: 327,
                top: 109,
                width: 127,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: Color::rgb(58, 52, 72),
                pressed_color: ACTIVE,
                text_color: MUTED,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::cycle_fps); }
            Button (
                "01",
                id: "pixel_frame_0",
                position: Position::Absolute,
                left: 230,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(0)); }
            Button (
                "02",
                id: "pixel_frame_1",
                position: Position::Absolute,
                left: 290,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(1)); }
            Button (
                "03",
                id: "pixel_frame_2",
                position: Position::Absolute,
                left: 350,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(2)); }
            Button (
                "04",
                id: "pixel_frame_3",
                position: Position::Absolute,
                left: 410,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(3)); }
            Button (
                "画笔",
                id: "pixel_tool_brush",
                position: Position::Absolute,
                left: 230,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, |model| model.set_tool(PixelTool::Brush)); }
            Button (
                "擦除",
                id: "pixel_tool_erase",
                position: Position::Absolute,
                left: 291,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, |model| model.set_tool(PixelTool::Erase)); }
            Button (
                "镜像",
                id: "pixel_tool_mirror",
                position: Position::Absolute,
                left: 352,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_mirror); }
            Button (
                "叠帧",
                id: "pixel_tool_onion",
                position: Position::Absolute,
                left: 413,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_onion); }
            Button (
                "",
                id: "pixel_color_1",
                position: Position::Absolute,
                left: 18,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[1],
                pressed_color: PALETTE[1],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(1)); }
            Button (
                "",
                id: "pixel_color_2",
                position: Position::Absolute,
                left: 51,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[2],
                pressed_color: PALETTE[2],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(2)); }
            Button (
                "",
                id: "pixel_color_3",
                position: Position::Absolute,
                left: 84,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[3],
                pressed_color: PALETTE[3],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(3)); }
            Button (
                "",
                id: "pixel_color_4",
                position: Position::Absolute,
                left: 117,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[4],
                pressed_color: PALETTE[4],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(4)); }
            Button (
                "",
                id: "pixel_color_5",
                position: Position::Absolute,
                left: 150,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[5],
                pressed_color: PALETTE[5],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(5)); }
            Button (
                "",
                id: "pixel_color_6",
                position: Position::Absolute,
                left: 183,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[6],
                pressed_color: PALETTE[6],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(6)); }
            Button (
                "复制上一帧",
                position: Position::Absolute,
                left: 230,
                top: 256,
                width: 146,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::copy_previous); }
            Button (
                "清空本帧",
                position: Position::Absolute,
                left: 382,
                top: 256,
                width: 86,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::open_clear); }
            Button (
                "模板",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 117,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::open_templates); }
            Button (
                "撤销",
                id: "pixel_undo",
                position: Position::Absolute,
                left: 136,
                top: 288,
                width: 117,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::undo); }
            Button (
                "播放动画",
                id: "pixel_play",
                position: Position::Absolute,
                left: 260,
                top: 288,
                width: 208,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_playback); }
            PixelModalSurface (
                id: "pixel_modal",
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
                    "先借一颗灵感",
                    id: "pixel_modal_title",
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
                    "载入模板会替换四帧；可以撤销，不会写入存储。",
                    id: "pixel_modal_subtitle",
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
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::close_modal); }
                Button (
                    "星际来客",
                    id: "pixel_template_0",
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
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(0)); }
                Button (
                    "风中绿芽",
                    id: "pixel_template_1",
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
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(1)); }
                Button (
                    "纸上飞行",
                    id: "pixel_template_2",
                    position: Position::Absolute,
                    left: 313,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(2)); }
                Text (
                    "当前帧预览",
                    id: "pixel_clear_preview",
                    position: Position::Absolute,
                    left: 57,
                    top: 211,
                    width: 96,
                    height: 16,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Button (
                    "清空这一帧",
                    id: "pixel_clear_confirm",
                    position: Position::Absolute,
                    left: 219,
                    top: 174,
                    width: 211,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACTIVE,
                    pressed_color: ACTIVE,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::confirm_clear); }
                Button (
                    "保留作品",
                    id: "pixel_clear_cancel",
                    position: Position::Absolute,
                    left: 219,
                    top: 214,
                    width: 211,
                    height: 27,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::close_modal); }
            }
        }
    };
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    app.world.insert_resource(PixelModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(pixel_tick_system::system());
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Pixel Loom node");
    let nodes = PixelNodes {
        surface: find("pixel_surface"),
        frame_status: find("pixel_frame_status"),
        mode_status: find("pixel_mode_status"),
        template_name: find("pixel_template_name"),
        fps: find("pixel_fps"),
        play: find("pixel_play"),
        undo: find("pixel_undo"),
        frames: core::array::from_fn(|index| {
            find(
                [
                    "pixel_frame_0",
                    "pixel_frame_1",
                    "pixel_frame_2",
                    "pixel_frame_3",
                ][index],
            )
        }),
        tools: core::array::from_fn(|index| {
            find(
                [
                    "pixel_tool_brush",
                    "pixel_tool_erase",
                    "pixel_tool_mirror",
                    "pixel_tool_onion",
                ][index],
            )
        }),
        colors: core::array::from_fn(|index| {
            find(
                [
                    "pixel_color_1",
                    "pixel_color_2",
                    "pixel_color_3",
                    "pixel_color_4",
                    "pixel_color_5",
                    "pixel_color_6",
                ][index],
            )
        }),
        modal: find("pixel_modal"),
        modal_title: find("pixel_modal_title"),
        modal_subtitle: find("pixel_modal_subtitle"),
        template_buttons: core::array::from_fn(|index| {
            find(["pixel_template_0", "pixel_template_1", "pixel_template_2"][index])
        }),
        clear_controls: [
            find("pixel_clear_preview"),
            find("pixel_clear_confirm"),
            find("pixel_clear_cancel"),
        ],
    };
    app.world.insert_resource(nodes);
    PixelNodes::sync(&mut app.world);
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
        assert!(app.world.find_by_id("pixel_surface").is_some());
        assert!(app.world.find_by_id("pixel_play").is_some());
        assert!(app.world.find_by_id("pixel_color_6").is_some());
        assert_eq!(app.world.query::<PixelSurface>().iter().count(), 1);
    }

    #[test]
    fn cancelled_drag_restores_the_whole_stroke() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        let surface = app.world.find_by_id("pixel_surface").unwrap();
        assert_eq!(
            local_cell(
                &app.world,
                surface,
                Fixed::from_int(16),
                Fixed::from_int(59)
            ),
            None
        );
        let before = *app.world.resource::<PixelModel>().unwrap().frames();
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragStart {
                x: Fixed::from_int(18),
                y: Fixed::from_int(60),
                target: surface,
            },
        );
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragMove {
                x: Fixed::from_int(193),
                y: Fixed::from_int(235),
                dx: Fixed::from_int(175),
                dy: Fixed::from_int(175),
                target: surface,
            },
        );
        surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragCancel {
                x: Fixed::from_int(193),
                y: Fixed::from_int(235),
                target: surface,
            },
        );
        assert_eq!(
            *app.world.resource::<PixelModel>().unwrap().frames(),
            before
        );
    }
}
