extern crate alloc;

use alloc::{format, string::String};

use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::tidal::{
    BOARD_SIZE, Perk, TideLevel, TideMessage, TideModal, TideModel, Tile,
};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BACKGROUND: Color = Color::rgb(23, 55, 64);
const HEADER: Color = Color::rgb(27, 63, 72);
const PANEL: Color = Color::rgb(32, 68, 75);
const BOARD: Color = Color::rgb(24, 61, 71);
const LINE: Color = Color::rgb(59, 91, 95);
const TEXT: Color = Color::rgb(231, 234, 215);
const MUTED: Color = Color::rgb(145, 170, 160);
const ACCENT: Color = Color::rgb(223, 186, 118);
const PAPER: Color = Color::rgb(225, 222, 196);
const DARK: Color = Color::rgb(36, 62, 64);
const CONTROL: Color = Color::rgb(42, 79, 84);

#[derive(crate::Component, Default)]
struct TideSurface;

#[derive(crate::Component, Default)]
struct TideModalSurface;

#[derive(Clone, Copy)]
struct TideNodes {
    surface: Entity,
    chapter: Entity,
    turn: Entity,
    total: Entity,
    forecast: Entity,
    estimate: Entity,
    until: Entity,
    offer_count: Entity,
    offers: [Entity; 3],
    info_title: Entity,
    info_desc: Entity,
    info_rule: Entity,
    preview: Entity,
    actions: [Entity; 5],
    goal: Entity,
    goal_progress: Entity,
    message: Entity,
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    modal_score: Entity,
    modal_detail: Entity,
    perk_buttons: [Entity; 3],
    finish: Entity,
    voyage_rows: [Entity; 4],
}

impl TideNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut TideModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<TideModel>()
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
        let Some(model) = world.resource::<TideModel>() else {
            return;
        };
        let forecast = model.forecast();
        let pending = model.pending();
        let selected = model.selected();
        let choice = model.choice();
        let modal = model.modal();
        let display_tile = selected
            .map(|index| model.tile(usize::from(index)))
            .unwrap_or_else(|| model.offer(usize::from(choice)));
        let preview =
            pending.and_then(|index| model.preview(usize::from(index), usize::from(choice)));
        let goal = model.goal();
        let texts = [
            (nodes.chapter, format!("第 {} / 4 岛", model.chapter() + 1)),
            (nodes.turn, format!("落子 {} / 24", model.turn())),
            (nodes.total, format!("累计 {}", model.cumulative_score())),
            (
                nodes.forecast,
                format!(
                    "{} · {}",
                    if forecast.tide == TideLevel::High {
                        "涨潮"
                    } else {
                        "退潮"
                    },
                    forecast.weather.name()
                ),
            ),
            (nodes.estimate, format!("预计 +{}", model.forecast_score())),
            (
                nodes.until,
                format!(
                    "再落 {} 块结算 · 进阶参考线 {}",
                    if model.settled() {
                        0
                    } else {
                        6 - model.turn() % 6
                    },
                    model.target()
                ),
            ),
            (nodes.offer_count, format!("换牌 {}", model.rerolls())),
            (
                nodes.info_title,
                format!(
                    "{} · {}",
                    if selected.is_some() {
                        "已落位"
                    } else {
                        "待落位"
                    },
                    display_tile.name()
                ),
            ),
            (nodes.info_desc, display_tile.description().into()),
            (nodes.info_rule, display_tile.rule().into()),
            (
                nodes.preview,
                preview.map_or_else(String::new, |(score, delta)| {
                    let terrain = model.terrain(usize::from(pending.expect("preview index")));
                    format!(
                        "{} · 本块 {} · 全岛净增 {:+}",
                        ["低地", "平地", "高地"][usize::from(terrain)],
                        score,
                        delta
                    )
                }),
            ),
            (nodes.goal, goal.name.into()),
            (
                nodes.goal_progress,
                format!("{} / {}", model.goal_progress(), goal.count),
            ),
            (nodes.message, message_text(model.message()).into()),
        ];
        let offers = [model.offer(0), model.offer(1), model.offer(2)];
        let modal_score = model.score();
        let harvests = (0..usize::from(model.harvest_len()))
            .map(|index| model.harvest(index))
            .fold(String::new(), |mut text, value| {
                if !text.is_empty() {
                    text.push_str(" / ");
                }
                use core::fmt::Write;
                let _ = write!(text, "{value}");
                text
            });
        let voyage_rows = core::array::from_fn::<_, 4, _>(|index| {
            if index < usize::from(model.chapter()) {
                let result = model.result(index);
                format!("0{}  {}  {}", index + 1, island_name(index), result.score)
            } else if index == usize::from(model.chapter()) {
                format!("0{}  {}  航行中", index + 1, island_name(index))
            } else {
                format!("0{}  {}  未抵达", index + 1, island_name(index))
            }
        });
        let is_result = modal == TideModal::Result;
        let is_voyage = modal == TideModal::Voyage;
        let final_island = model.chapter() == 3;
        let settled = model.settled();
        let complete = model.complete();
        let history = model.history_len();
        let _ = model;

        for (entity, content) in texts {
            set_text(world, entity, content);
        }
        for (index, entity) in nodes.offers.into_iter().enumerate() {
            set_text(world, entity, offers[index].name());
            set_button_state(
                world,
                entity,
                index == usize::from(choice),
                !settled && !complete,
            );
        }
        set_button_state(
            world,
            nodes.actions[0],
            false,
            !settled && !complete && rerolls(world) > 0,
        );
        set_button_state(world, nodes.actions[1], false, history > 0 && !complete);
        set_hidden(
            world,
            nodes.actions[0],
            pending.is_some() || settled || complete,
        );
        set_hidden(
            world,
            nodes.actions[1],
            pending.is_some() || settled || complete,
        );
        set_hidden(
            world,
            nodes.actions[2],
            pending.is_some() || settled || complete,
        );
        set_hidden(world, nodes.actions[3], pending.is_none());
        set_hidden(
            world,
            nodes.actions[4],
            pending.is_none() && !settled && !complete,
        );
        if pending.is_none() && (settled || complete) {
            set_text(
                world,
                nodes.actions[4],
                if complete {
                    "航行完成 · 查看总览"
                } else {
                    "本岛结算 · 选择新学说"
                },
            );
        } else {
            set_text(world, nodes.actions[4], "取消");
        }

        set_hidden(world, nodes.modal, modal == TideModal::None);
        set_text(
            world,
            nodes.modal_title,
            if is_voyage {
                "四岛航行图"
            } else if complete {
                "四岛远航 / 完成"
            } else {
                "本岛结算"
            },
        );
        set_text(
            world,
            nodes.modal_subtitle,
            if is_voyage {
                "96 次落子 / 16 次季节结算 / 3 次学说选择"
            } else {
                "评级不锁关。选择学说，继续前往下一座岛。"
            },
        );
        set_text(
            world,
            nodes.modal_score,
            if is_result {
                format!("{modal_score}")
            } else {
                String::new()
            },
        );
        set_text(
            world,
            nodes.modal_detail,
            if is_result {
                format!("四季 {harvests}")
            } else {
                String::new()
            },
        );
        set_hidden(world, nodes.modal_score, !is_result);
        set_hidden(world, nodes.modal_detail, !is_result);
        for (index, entity) in nodes.perk_buttons.into_iter().enumerate() {
            let visible = is_result && settled && !final_island && !complete;
            set_hidden(world, entity, !visible);
            if visible {
                let perk = perk_offer(world, index);
                set_text(
                    world,
                    entity,
                    format!("{}\n{}", perk.name(), perk.description()),
                );
            }
        }
        set_hidden(
            world,
            nodes.finish,
            !(is_result && final_island && settled && !complete),
        );
        for (index, entity) in nodes.voyage_rows.into_iter().enumerate() {
            set_hidden(world, entity, !is_voyage);
            if is_voyage {
                set_text(world, entity, voyage_rows[index].clone());
            }
        }
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<alloc::string::String>) {
    if let Some(text) = world.get_mut::<Text>(entity) {
        text.set_content(content.into());
    }
    world.invalidate(entity);
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
            PAPER.into()
        } else if enabled {
            PANEL.into()
        } else {
            Color::rgb(29, 57, 63).into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            DARK.into()
        } else if enabled {
            TEXT.into()
        } else {
            MUTED.into()
        };
    }
    world.invalidate_visual(entity);
}

fn rerolls(world: &World) -> u8 {
    world.resource::<TideModel>().map_or(0, TideModel::rerolls)
}
fn perk_offer(world: &World, index: usize) -> Perk {
    world
        .resource::<TideModel>()
        .expect("Tide model")
        .perk_offer(index)
}

fn message_text(message: TideMessage) -> &'static str {
    match message {
        TideMessage::Ready => "选一张地块，再点相邻海域。每 6 次落子结算一次。",
        TideMessage::Placed(_) => "地块已落位，候选与下一季预估已更新。",
        TideMessage::Harvest(_, _) => "季节结算完成；继续扩建群岛。",
        TideMessage::Rerolled => "换一手地块；不会消耗落子回合。",
        TideMessage::Undone => "已撤销；候选、积分与随机状态完整恢复。",
        TideMessage::Settled(true) => "委托达成，追加 45 分。",
        TideMessage::Settled(false) => "本岛结算；未完成委托不影响继续远航。",
        TideMessage::Complete => "四岛航行完成。",
    }
}

fn island_name(index: usize) -> &'static str {
    ["浅湾", "外海", "浮岬", "远境"][index]
}

fn tile_color(tile: Tile) -> Color {
    match tile {
        Tile::Sea => BOARD,
        Tile::Grove => Color::rgb(120, 165, 140),
        Tile::Field => Color::rgb(214, 182, 110),
        Tile::Hamlet => Color::rgb(204, 135, 115),
        Tile::Harbor => Color::rgb(115, 165, 181),
        Tile::Lens => Color::rgb(169, 154, 195),
        Tile::Lagoon => Color::rgb(95, 147, 155),
        Tile::Beacon => Color::rgb(210, 203, 166),
        Tile::Dike => Color::rgb(149, 157, 154),
    }
}

fn paint_tile(painter: &mut PlayPainter<'_, '_>, tile: Tile, center: Point) {
    let color = tile_color(tile);
    let x = center.x.to_int();
    let y = center.y.to_int();
    match tile {
        Tile::Sea => {}
        Tile::Grove => {
            painter.line(
                Point::new(x, y + 8),
                Point::new(x, y - 2),
                color,
                Fixed::ONE,
            );
            painter.fill(Rect::new(x - 7, y - 6, 14, 11), color, Fixed::from_int(7));
        }
        Tile::Field => {
            for offset in [-6, -1, 4] {
                painter.line(
                    Point::new(x - 8 + offset / 2, y + 7),
                    Point::new(x + offset, y - 7),
                    color,
                    Fixed::ONE,
                );
            }
            painter.line(
                Point::new(x - 9, y + 8),
                Point::new(x + 9, y + 8),
                color,
                Fixed::ONE,
            );
        }
        Tile::Hamlet => {
            painter.fill(Rect::new(x - 7, y - 1, 14, 11), color, Fixed::ONE);
            painter.line(
                Point::new(x - 9, y - 1),
                Point::new(x, y - 10),
                color,
                Fixed::from_int(2),
            );
            painter.line(
                Point::new(x, y - 10),
                Point::new(x + 9, y - 1),
                color,
                Fixed::from_int(2),
            );
        }
        Tile::Harbor => {
            painter.line(
                Point::new(x - 9, y + 5),
                Point::new(x + 9, y + 5),
                color,
                Fixed::from_int(2),
            );
            painter.line(
                Point::new(x, y + 5),
                Point::new(x, y - 10),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x, y - 9),
                Point::new(x + 8, y - 1),
                color,
                Fixed::ONE,
            );
        }
        Tile::Lens => {
            painter.arc(
                center,
                Fixed::from_int(9),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - 7, y + 7),
                Point::new(x + 7, y - 7),
                color,
                Fixed::ONE,
            );
            painter.circle(center, Fixed::from_int(2), color);
        }
        Tile::Lagoon => {
            for offset in [-6, 0, 6] {
                painter.line(
                    Point::new(x - 9, y + offset),
                    Point::new(x - 3, y + offset - 2),
                    color,
                    Fixed::ONE,
                );
                painter.line(
                    Point::new(x - 3, y + offset - 2),
                    Point::new(x + 3, y + offset + 2),
                    color,
                    Fixed::ONE,
                );
                painter.line(
                    Point::new(x + 3, y + offset + 2),
                    Point::new(x + 9, y + offset),
                    color,
                    Fixed::ONE,
                );
            }
        }
        Tile::Beacon => {
            painter.fill(Rect::new(x - 4, y - 5, 8, 14), color, Fixed::ONE);
            painter.fill(Rect::new(x - 7, y - 10, 14, 5), color, Fixed::from_int(2));
        }
        Tile::Dike => {
            for row in 0..3 {
                painter.line(
                    Point::new(x - 9 + row * 2, y - 7 + row * 6),
                    Point::new(x + 8, y - 7 + row * 6),
                    color,
                    Fixed::from_int(2),
                );
            }
        }
    }
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &TideModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(Point::new(14, 34), Point::new(466, 34), LINE, Fixed::ONE);
    painter.fill(Rect::new(237, 60, 229, 51), PANEL, Fixed::from_int(4));
    painter.border(
        Rect::new(237, 60, 229, 51),
        LINE,
        Fixed::ONE,
        Fixed::from_int(4),
    );
    let forecast = model.forecast();
    for index in 0..BOARD_SIZE {
        let x = 14 + (index % 6) as i32 * 35;
        let y = 60 + (index / 6) as i32 * 35;
        let tile = model.tile(index);
        let legal = model.valid(index) && !model.settled() && !model.complete();
        painter.fill(
            Rect::new(x, y, 32, 32),
            if tile == Tile::Sea {
                if model.terrain(index) == 0 {
                    Color::rgb(32, 74, 84)
                } else {
                    Color::rgb(28, 64, 73)
                }
            } else {
                Color::rgb(51, 86, 89)
            },
            Fixed::from_int(3),
        );
        painter.border(
            Rect::new(x, y, 32, 32),
            if legal {
                Color::rgb(82, 119, 117)
            } else {
                LINE
            },
            Fixed::ONE,
            Fixed::from_int(3),
        );
        if tile != Tile::Sea {
            paint_tile(painter, tile, Point::new(x + 16, y + 16));
            if matches!(tile, Tile::Field | Tile::Hamlet) && model.tile_score(index, forecast) == 0
            {
                painter.line(
                    Point::new(x + 4, y + 27),
                    Point::new(x + 28, y + 27),
                    Color::rgb(142, 186, 202),
                    Fixed::from_int(2),
                );
            }
        } else if model.terrain(index) == 2 {
            painter.line(
                Point::new(x + 11, y + 21),
                Point::new(x + 17, y + 11),
                Color::rgb(82, 115, 112),
                Fixed::ONE,
            );
            painter.line(
                Point::new(x + 17, y + 11),
                Point::new(x + 23, y + 21),
                Color::rgb(82, 115, 112),
                Fixed::ONE,
            );
        }
        for mark in 0..=model.terrain(index) {
            painter.fill(
                Rect::new(x + 3 + i32::from(mark) * 4, y + 3, 2, 2),
                Color::rgb(165, 181, 160),
                Fixed::ZERO,
            );
        }
        if model.selected() == Some(index as u8) || model.pending() == Some(index as u8) {
            painter.border(
                Rect::new(x + 1, y + 1, 30, 30),
                ACCENT,
                Fixed::from_int(2),
                Fixed::from_int(3),
            );
            if tile == Tile::Sea {
                paint_tile(
                    painter,
                    model.offer(usize::from(model.choice())),
                    Point::new(x + 16, y + 16),
                );
            }
        }
    }
    painter.line(Point::new(14, 296), Point::new(466, 296), LINE, Fixed::ONE);
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<TideModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_surface(&mut painter, model);
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<TideModel>() else {
        return;
    };
    if model.modal() == TideModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 34, 480, 264),
        Color::rgba(16, 38, 45, 236),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(20, 43, 440, 247), PANEL, Fixed::from_int(7));
    painter.border(
        Rect::new(20, 43, 440, 247),
        LINE,
        Fixed::ONE,
        Fixed::from_int(7),
    );
    painter.line(Point::new(35, 96), Point::new(445, 96), LINE, Fixed::ONE);
}

fn surface_view() -> View {
    View::new("TideSurface", 60, surface_render).with_filter::<TideSurface>()
}
fn modal_view() -> View {
    View::new("TideModalSurface", 70, modal_render).with_filter::<TideModalSurface>()
}

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    if local_x < Fixed::from_int(14)
        || local_x >= Fixed::from_int(224)
        || local_y < Fixed::from_int(60)
        || local_y >= Fixed::from_int(270)
    {
        return None;
    }
    let column = (local_x.to_int() - 14) / 35;
    let row = (local_y.to_int() - 60) / 35;
    if (local_x.to_int() - 14) % 35 >= 32 || (local_y.to_int() - 60) % 35 >= 32 {
        return None;
    }
    Some((row * 6 + column) as u8)
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(cell) = local_cell(world, entity, *x, *y) else {
        return false;
    };
    TideNodes::update(world, |model| model.select_cell(cell));
    true
}

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
fn build_widgets() {
    ui! {
        TideSurface (id: "tide_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "潮汐群岛",
                position: Position::Absolute,
                left: 14,
                top: 7,
                width: 105,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "TIDAL ATLAS / 004096",
                position: Position::Absolute,
                left: 123,
                top: 10,
                width: 190,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "第 1 / 4 岛",
                id: "tide_chapter",
                position: Position::Absolute,
                left: 14,
                top: 39,
                width: 90,
                height: 16,
                font_size: 9,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "落子 0 / 24",
                id: "tide_turn",
                position: Position::Absolute,
                left: 115,
                top: 39,
                width: 90,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "累计 0",
                id: "tide_total",
                position: Position::Absolute,
                left: 171,
                top: 39,
                width: 85,
                height: 16,
                font_size: 9,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "NEXT HARVEST",
                position: Position::Absolute,
                left: 247,
                top: 67,
                width: 92,
                height: 11,
                font_size: 7,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "退潮 · 晴朗",
                id: "tide_forecast",
                position: Position::Absolute,
                left: 247,
                top: 80,
                width: 120,
                height: 18,
                font_size: 12,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "预计 +0",
                id: "tide_estimate",
                position: Position::Absolute,
                left: 366,
                top: 80,
                width: 90,
                height: 18,
                font_size: 11,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "再落 6 块结算",
                id: "tide_until",
                position: Position::Absolute,
                left: 247,
                top: 98,
                width: 209,
                height: 11,
                font_size: 7,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "选择地块",
                position: Position::Absolute,
                left: 237,
                top: 116,
                width: 90,
                height: 15,
                font_size: 9,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "换牌 3",
                id: "tide_offer_count",
                position: Position::Absolute,
                left: 390,
                top: 116,
                width: 76,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "森林",
                id: "tide_offer_0",
                position: Position::Absolute,
                left: 237,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PAPER,
                pressed_color: ACCENT,
                text_color: DARK,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(0)); }
            Button (
                "梯田",
                id: "tide_offer_1",
                position: Position::Absolute,
                left: 315,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(1)); }
            Button (
                "港湾",
                id: "tide_offer_2",
                position: Position::Absolute,
                left: 393,
                top: 135,
                width: 73,
                height: 58,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { TideNodes::update(ctx.world, |model| model.select_offer(2)); }
            Text (
                "待落位",
                id: "tide_info_title",
                position: Position::Absolute,
                left: 237,
                top: 199,
                width: 220,
                height: 15,
                font_size: 9,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "基础规则",
                id: "tide_info_desc",
                position: Position::Absolute,
                left: 237,
                top: 214,
                width: 220,
                height: 14,
                font_size: 8,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "相邻规则",
                id: "tide_info_rule",
                position: Position::Absolute,
                left: 237,
                top: 228,
                width: 220,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "",
                id: "tide_preview",
                position: Position::Absolute,
                left: 237,
                top: 244,
                width: 229,
                height: 12,
                font_size: 7,
                text_color: ACCENT,
                paragraph: label()
            )
            Button (
                "换牌",
                id: "tide_reroll",
                position: Position::Absolute,
                left: 237,
                top: 252,
                width: 75,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap { TideNodes::update(ctx.world, TideModel::reroll); }
            Button (
                "撤销",
                id: "tide_undo",
                position: Position::Absolute,
                left: 318,
                top: 252,
                width: 66,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap { TideNodes::update(ctx.world, TideModel::undo); }
            Button (
                "航行图",
                id: "tide_voyage",
                position: Position::Absolute,
                left: 390,
                top: 252,
                width: 76,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap { TideNodes::update(ctx.world, |model| model.set_modal(TideModal::Voyage)); }
            Button (
                "确认落子",
                id: "tide_place",
                position: Position::Absolute,
                left: 237,
                top: 258,
                width: 151,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: PAPER,
                text_color: DARK,
                border_radius: 5
            ) on Tap { TideNodes::update(ctx.world, TideModel::place_pending); }
            Button (
                "取消",
                id: "tide_cancel_result",
                position: Position::Absolute,
                left: 397,
                top: 258,
                width: 69,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 5
            ) on Tap {
                let show_result = ctx
                    .world
                    .resource::<TideModel>()
                    .is_some_and(|model| model.settled() || model.complete());
                TideNodes::update(
                    ctx.world,
                    |model| {
                        if show_result {
                            model.set_modal(TideModal::Result)
                        } else {
                            model.cancel_preview()
                        }
                    },
                );
            }
            Text (
                "委托",
                id: "tide_goal",
                position: Position::Absolute,
                left: 14,
                top: 274,
                width: 140,
                height: 15,
                font_size: 9,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "0 / 4",
                id: "tide_goal_progress",
                position: Position::Absolute,
                left: 164,
                top: 274,
                width: 53,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "准备远航",
                id: "tide_message",
                position: Position::Absolute,
                left: 14,
                top: 300,
                width: 452,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: label()
            )
            TideModalSurface (
                id: "tide_modal",
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
                    "本岛结算",
                    id: "tide_modal_title",
                    position: Position::Absolute,
                    left: 35,
                    top: 53,
                    width: 370,
                    height: 24,
                    font_size: 15,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_modal_subtitle",
                    position: Position::Absolute,
                    left: 35,
                    top: 77,
                    width: 385,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label()
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 421,
                    top: 50,
                    width: 26,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 14,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 5
                ) on Tap { TideNodes::update(ctx.world, |model| model.set_modal(TideModal::None)); }
                Text (
                    "0",
                    id: "tide_modal_score",
                    position: Position::Absolute,
                    left: 35,
                    top: 111,
                    width: 130,
                    height: 45,
                    font_size: 34,
                    text_color: ACCENT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_modal_detail",
                    position: Position::Absolute,
                    left: 180,
                    top: 120,
                    width: 250,
                    height: 24,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label()
                )
                Button (
                    "学说一",
                    id: "tide_perk_0",
                    position: Position::Absolute,
                    left: 35,
                    top: 181,
                    width: 132,
                    height: 86,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: BACKGROUND,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 5
                ) on Tap {
                    let perk = perk_offer(ctx.world, 0);
                    TideNodes::update(ctx.world, |model| model.continue_voyage(Some(perk)));
                }
                Button (
                    "学说二",
                    id: "tide_perk_1",
                    position: Position::Absolute,
                    left: 174,
                    top: 181,
                    width: 132,
                    height: 86,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: BACKGROUND,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 5
                ) on Tap {
                    let perk = perk_offer(ctx.world, 1);
                    TideNodes::update(ctx.world, |model| model.continue_voyage(Some(perk)));
                }
                Button (
                    "学说三",
                    id: "tide_perk_2",
                    position: Position::Absolute,
                    left: 313,
                    top: 181,
                    width: 132,
                    height: 86,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: BACKGROUND,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 5
                ) on Tap {
                    let perk = perk_offer(ctx.world, 2);
                    TideNodes::update(ctx.world, |model| model.continue_voyage(Some(perk)));
                }
                Button (
                    "完成四岛航行",
                    id: "tide_finish",
                    position: Position::Absolute,
                    left: 35,
                    top: 225,
                    width: 410,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACCENT,
                    pressed_color: PAPER,
                    text_color: DARK,
                    border_radius: 6
                ) on Tap { TideNodes::update(ctx.world, |model| model.continue_voyage(None)); }
                Text (
                    "",
                    id: "tide_voyage_0",
                    position: Position::Absolute,
                    left: 45,
                    top: 112,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_1",
                    position: Position::Absolute,
                    left: 45,
                    top: 148,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_2",
                    position: Position::Absolute,
                    left: 45,
                    top: 184,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "",
                    id: "tide_voyage_3",
                    position: Position::Absolute,
                    left: 45,
                    top: 220,
                    width: 390,
                    height: 28,
                    font_size: 12,
                    text_color: TEXT,
                    paragraph: label()
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
    register_play_font(&mut app.world);
    app.world.insert_resource(TideModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Tidal Atlas node");
    let nodes = TideNodes {
        surface: find("tide_surface"),
        chapter: find("tide_chapter"),
        turn: find("tide_turn"),
        total: find("tide_total"),
        forecast: find("tide_forecast"),
        estimate: find("tide_estimate"),
        until: find("tide_until"),
        offer_count: find("tide_offer_count"),
        offers: [
            find("tide_offer_0"),
            find("tide_offer_1"),
            find("tide_offer_2"),
        ],
        info_title: find("tide_info_title"),
        info_desc: find("tide_info_desc"),
        info_rule: find("tide_info_rule"),
        preview: find("tide_preview"),
        actions: [
            find("tide_reroll"),
            find("tide_undo"),
            find("tide_voyage"),
            find("tide_place"),
            find("tide_cancel_result"),
        ],
        goal: find("tide_goal"),
        goal_progress: find("tide_goal_progress"),
        message: find("tide_message"),
        modal: find("tide_modal"),
        modal_title: find("tide_modal_title"),
        modal_subtitle: find("tide_modal_subtitle"),
        modal_score: find("tide_modal_score"),
        modal_detail: find("tide_modal_detail"),
        perk_buttons: [
            find("tide_perk_0"),
            find("tide_perk_1"),
            find("tide_perk_2"),
        ],
        finish: find("tide_finish"),
        voyage_rows: [
            find("tide_voyage_0"),
            find("tide_voyage_1"),
            find("tide_voyage_2"),
            find("tide_voyage_3"),
        ],
    };
    app.world.insert_resource(nodes);
    TideNodes::sync(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_uses_one_dense_board_and_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("tide_surface").is_some());
        assert!(app.world.find_by_id("tide_offer_0").is_some());
        assert!(app.world.find_by_id("tide_place").is_some());
        assert_eq!(app.world.query::<TideSurface>().iter().count(), 1);
    }

    #[test]
    fn board_hit_testing_rejects_cell_gaps() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        app.render().unwrap();
        let surface = app.world.find_by_id("tide_surface").unwrap();
        assert_eq!(
            local_cell(
                &app.world,
                surface,
                Fixed::from_int(14),
                Fixed::from_int(60)
            ),
            Some(0)
        );
        assert_eq!(
            local_cell(
                &app.world,
                surface,
                Fixed::from_int(46),
                Fixed::from_int(60)
            ),
            None
        );
    }
}
