//! Echo Walker is a deterministic route-recording puzzle with fixed-capacity state.

use alloc::{format, string::String};

use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::echo::{
    BEAT_LIMIT, BOARD_HEIGHT, BOARD_WIDTH, Direction, EchoCommand, EchoMessage, EchoModal,
    EchoModel,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::surface::InputEvent;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BACKGROUND: Color = Color::rgb(30, 34, 57);
const HEADER: Color = Color::rgb(35, 40, 66);
const PANEL: Color = Color::rgb(39, 47, 70);
const BOARD: Color = Color::rgb(37, 50, 73);
const WALL: Color = Color::rgb(48, 55, 77);
const LINE: Color = Color::rgb(62, 72, 99);
const TEXT: Color = Color::rgb(239, 235, 220);
const MUTED: Color = Color::rgb(148, 155, 181);
const ACCENT: Color = Color::rgb(220, 190, 132);
const TEAL: Color = Color::rgb(124, 190, 174);
const VIOLET: Color = Color::rgb(190, 166, 206);
const RUST: Color = Color::rgb(199, 161, 123);

#[derive(crate::Component, Default)]
struct EchoSurface;

#[derive(crate::Component, Default)]
struct EchoModalSurface;

#[derive(Clone, Copy)]
struct EchoNodes {
    surface: Entity,
    room: Entity,
    memory: Entity,
    ghosts: Entity,
    tick: Entity,
    run: Entity,
    message: Entity,
    actions: [Entity; 4],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    modal_primary: Entity,
    modal_secondary: Entity,
    tape_rows: [Entity; 3],
    result_stats: Entity,
}

impl EchoNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut EchoModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<EchoModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        Self::apply_changes(world, changes);
    }

    fn dispatch(world: &mut World, command: EchoCommand) {
        #[cfg(feature = "persistence")]
        if world
            .resource::<EchoReplayLog>()
            .is_none_or(EchoReplayLog::is_full)
        {
            return;
        }
        let changes = world
            .resource_mut::<EchoModel>()
            .map(|model| model.apply_command(command))
            .unwrap_or(ChangeSet::NONE);
        #[cfg(feature = "persistence")]
        if changes.contains(ChangeSet::PERSISTENCE) {
            let recorded = world
                .resource_mut::<EchoReplayLog>()
                .is_some_and(|log| log.record_echo(command).is_ok());
            debug_assert!(recorded);
        }
        Self::apply_changes(world, changes);
    }

    fn apply_changes(world: &mut World, changes: ChangeSet) {
        if changes.contains(ChangeSet::VISUAL)
            && let Some(nodes) = world.resource::<Self>().copied()
        {
            world.invalidate_visual(nodes.surface);
            world.invalidate_visual(nodes.modal);
        }
        if changes.contains(ChangeSet::MODEL) {
            Self::sync(world);
        }
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<EchoModel>() else {
            return;
        };
        let modal = model.modal();
        let complete = model.complete();
        let won = model.won();
        let ghost_count = model.ghost_count();
        let route_len = model.route_len();
        let history_len = model.history_len();
        let peek = model.peek_ghost();
        let result_len = model.result_len();
        let result_stats = if complete {
            format!(
                "TOTAL {} STEPS · {} REWINDS",
                model.total_steps(),
                model.total_loops()
            )
        } else {
            format!("ROOM {} STEPS · {} REWINDS", model.steps(), model.loops())
        };
        let texts = [
            (nodes.room, format!("ARCHIVE {:02} / 12", model.level() + 1)),
            (
                nodes.memory,
                format!(
                    "MEMORY {}/{}",
                    model.collected_count(),
                    model.room().gem_count
                ),
            ),
            (nodes.ghosts, format!("ECHOES {ghost_count}/3")),
            (nodes.tick, format!("{:02}", model.tick())),
            (
                nodes.run,
                format!("{} STEPS / {} ECHOES", model.steps(), model.loops()),
            ),
            (nodes.message, message_text(model.message())),
            (nodes.result_stats, result_stats),
        ];
        let _ = model;
        for (entity, content) in texts {
            set_text(world, entity, content);
        }
        set_button_state(
            world,
            nodes.actions[0],
            route_len > 0 && ghost_count < 3 && !won,
        );
        set_button_state(world, nodes.actions[1], history_len > 0 && !complete);
        set_button_state(world, nodes.actions[2], route_len > 0 && !won);
        set_button_state(world, nodes.actions[3], true);
        set_hidden(world, nodes.modal, modal == EchoModal::None);
        let tapes = modal == EchoModal::Tapes;
        let result = modal == EchoModal::Result;
        set_text(
            world,
            nodes.modal_title,
            if tapes {
                "ECHO TAPES"
            } else if complete {
                "TWELVE ROOMS RESTORED"
            } else {
                "ARCHIVE RESTORED"
            },
        );
        set_text(
            world,
            nodes.modal_subtitle,
            if tapes {
                "Inspect each route. An echo holds its final cell."
            } else if complete {
                "All twelve rooms are restored."
            } else {
                "Past and present completed the route together."
            },
        );
        for (index, row) in nodes.tape_rows.into_iter().enumerate() {
            set_hidden(world, row, !tapes || index >= usize::from(ghost_count));
            set_text(
                world,
                row,
                format!(
                    "ECHO {} · {:02} BEATS{}",
                    index + 1,
                    ghost_route_len(world, index),
                    if peek == Some(index as u8) {
                        " · SOLO"
                    } else {
                        ""
                    }
                ),
            );
            set_button_state(world, row, true);
        }
        set_hidden(world, nodes.result_stats, !result);
        set_hidden(world, nodes.modal_primary, !result);
        set_hidden(world, nodes.modal_secondary, !tapes);
        set_text(
            world,
            nodes.modal_primary,
            if complete {
                "ARCHIVE COMPLETE"
            } else if result_len == 11 {
                "COMPLETE ARCHIVE"
            } else {
                "NEXT ROOM"
            },
        );
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<String>) {
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

fn set_button_state(world: &mut World, entity: Entity, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if enabled { PANEL } else { HEADER }.into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if enabled { TEXT } else { MUTED }.into();
    }
    world.invalidate_visual(entity);
}

fn ghost_route_len(world: &World, ghost: usize) -> u8 {
    world
        .resource::<EchoModel>()
        .map_or(0, |model| model.ghost_route_len(ghost))
}

fn message_text(message: EchoMessage) -> String {
    match message {
        EchoMessage::Ready => "Reach a plate, rewind, and let the past replay the route.".into(),
        EchoMessage::Plate(index) => format!(
            "PLATE {} HELD · RECORD THIS ROUTE",
            char::from(b'A' + index)
        ),
        EchoMessage::Fragment => "MEMORY FRAGMENT STORED ACROSS REWINDS".into(),
        EchoMessage::Missing(count) => format!("EXIT NEEDS {count} MORE MEMORY FRAGMENTS"),
        EchoMessage::BeatLimit => "48 BEATS USED · RECORD OR RESTART THIS ROUTE".into(),
        EchoMessage::Recorded => "ECHO RECORDED · IT WILL HOLD THE FINAL CELL".into(),
        EchoMessage::Restarted => "ROUTE RESTARTED · ECHOES AND MEMORY REMAIN".into(),
        EchoMessage::Cleared => "ROOM RESET · SEED AND GEOMETRY PRESERVED".into(),
        EchoMessage::Undone => "UNDO RESTORED TIME, MEMORY, AND ECHOES".into(),
        EchoMessage::Won => "ARCHIVE RESTORED · CONTINUE TO THE NEXT ROOM".into(),
        EchoMessage::Complete => "ALL TWELVE ROOMS RESTORED".into(),
    }
}

#[derive(Clone, Copy)]
struct MapBox {
    min: usize,
    max: usize,
    size: i32,
    x: i32,
    y: i32,
}

fn map_box(model: &EchoModel) -> MapBox {
    let room = model.room();
    let mut min = BOARD_WIDTH - 1;
    let mut max = 0;
    for cell in 0..BOARD_WIDTH * BOARD_HEIGHT {
        if !room.is_wall(cell) {
            let x = cell % BOARD_WIDTH;
            min = min.min(x);
            max = max.max(x);
        }
    }
    min = min.saturating_sub(1);
    max = (max + 1).min(BOARD_WIDTH - 1);
    let columns = max - min + 1;
    let size = (278 / columns as i32).min(29);
    MapBox {
        min,
        max,
        size,
        x: 13 + (278 - columns as i32 * size) / 2,
        y: 64,
    }
}

fn cell_origin(cell: u8, map: MapBox) -> (i32, i32) {
    let x = usize::from(cell) % BOARD_WIDTH;
    let y = usize::from(cell) / BOARD_WIDTH;
    (
        map.x + (x - map.min) as i32 * map.size,
        map.y + y as i32 * map.size,
    )
}

fn paint_surface(painter: &mut PlayPainter<'_, '_>, model: &EchoModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(Point::new(14, 34), Point::new(466, 34), LINE, Fixed::ONE);
    let map = map_box(model);
    let room = model.room();
    for cell in 0..BOARD_WIDTH * BOARD_HEIGHT {
        let x = cell % BOARD_WIDTH;
        if x < map.min || x > map.max {
            continue;
        }
        let (left, top) = cell_origin(cell as u8, map);
        let area = Rect::new(left + 1, top + 1, map.size - 2, map.size - 2);
        if room.is_wall(cell) {
            painter.fill(area, WALL, Fixed::ONE);
            painter.line(
                Point::new(left + 4, top + map.size - 5),
                Point::new(left + map.size - 4, top + map.size - 5),
                LINE,
                Fixed::ONE,
            );
            continue;
        }
        painter.fill(area, BOARD, Fixed::ONE);
        painter.border(area, LINE, Fixed::ONE, Fixed::ONE);
        let center = Point::new(left + map.size / 2, top + map.size / 2);
        if let Some(door) = room.door_at(cell as u8) {
            let open = model.door_open(door, model.tick(), model.position());
            painter.fill(
                Rect::new(left + 3, top + 3, map.size - 6, map.size - 6),
                if open {
                    Color::rgb(43, 77, 83)
                } else {
                    Color::rgb(112, 93, 88)
                },
                Fixed::ONE,
            );
            let color = if open { TEAL } else { RUST };
            for bar in 0..3 {
                painter.line(
                    Point::new(left + 6 + bar * 5, top + 5),
                    Point::new(left + 6 + bar * 5, top + map.size - 5),
                    color,
                    Fixed::ONE,
                );
            }
        }
        if let Some(plate) = room.plate_at(cell as u8) {
            painter.circle(center, Fixed::from_int(map.size / 3), VIOLET);
            painter.circle(center, Fixed::from_int(map.size / 5), BOARD);
            if model.door_open(plate, model.tick(), model.position()) {
                painter.arc(
                    center,
                    Fixed::from_int(map.size / 2 - 2),
                    Fixed::ZERO,
                    Fixed::from_int(360),
                    TEAL,
                    Fixed::ONE,
                );
            }
        }
        if cell as u8 == room.start {
            painter.circle(center, Fixed::from_int(4), MUTED);
        }
        if cell as u8 == room.end {
            painter.border(
                Rect::new(left + 6, top + 6, map.size - 12, map.size - 12),
                ACCENT,
                Fixed::ONE,
                Fixed::ONE,
            );
        }
        if let Some(gem) = room.gem_at(cell as u8)
            && !model.collected(gem)
        {
            painter.circle(center, Fixed::from_int(5), ACCENT);
            painter.circle(center, Fixed::from_int(2), BACKGROUND);
        }
        if room.clock_at(cell as u8).is_some() {
            let open = model.clock_open(cell as u8, model.tick() + 1);
            let color = if open {
                TEAL
            } else {
                Color::rgb(177, 121, 131)
            };
            painter.circle(center, Fixed::from_int(7), BOARD);
            painter.arc(
                center,
                Fixed::from_int(7),
                Fixed::ZERO,
                Fixed::from_int(360),
                color,
                Fixed::ONE,
            );
            painter.line(
                center,
                Point::new(center.x, center.y - Fixed::from_int(5)),
                color,
                Fixed::ONE,
            );
            painter.line(
                center,
                Point::new(center.x + Fixed::from_int(4), center.y),
                color,
                Fixed::ONE,
            );
        }
    }
    let colors = [TEAL, VIOLET, RUST];
    for (ghost, color) in colors
        .iter()
        .copied()
        .enumerate()
        .take(usize::from(model.ghost_count()))
    {
        if model
            .peek_ghost()
            .is_some_and(|peek| usize::from(peek) != ghost)
        {
            continue;
        }
        let len = model.ghost_route_len(ghost);
        for beat in 1..=len {
            let a = model.route_cell(ghost + 1, beat - 1);
            let b = model.route_cell(ghost + 1, beat);
            let (ax, ay) = cell_origin(a, map);
            let (bx, by) = cell_origin(b, map);
            painter.line(
                Point::new(ax + map.size / 2, ay + map.size / 2),
                Point::new(bx + map.size / 2, by + map.size / 2),
                color,
                Fixed::ONE,
            );
        }
        let cell = model.ghost_position(ghost, model.tick());
        let (left, top) = cell_origin(cell, map);
        let center = Point::new(left + map.size / 2, top + map.size / 2);
        painter.circle(center, Fixed::from_int(map.size / 3), colors[ghost]);
        painter.circle(center, Fixed::from_int(map.size / 5), BOARD);
    }
    let (left, top) = cell_origin(model.position(), map);
    let center = Point::new(left + map.size / 2, top + map.size / 2);
    painter.circle(center, Fixed::from_int(7), TEXT);
    painter.circle(center, Fixed::from_int(3), BACKGROUND);
    for beat in 0..BEAT_LIMIT {
        painter.fill(
            Rect::new(14 + i32::from(beat) * 575 / 100, 274, 4, 5),
            if beat < model.tick() {
                ACCENT
            } else {
                Color::rgb(71, 81, 105)
            },
            Fixed::ONE,
        );
    }
    painter.line(Point::new(14, 298), Point::new(466, 298), LINE, Fixed::ONE);
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<EchoModel>() else {
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
    let Some(model) = world.resource::<EchoModel>() else {
        return;
    };
    if model.modal() == EchoModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 34, 480, 264),
        Color::rgba(24, 27, 48, 239),
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
    View::new("EchoSurface", 60, surface_render).with_filter::<EchoSurface>()
}

fn modal_view() -> View {
    View::new("EchoModalSurface", 70, modal_render).with_filter::<EchoModalSurface>()
}

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    let model = world.resource::<EchoModel>()?;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int();
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int();
    let map = map_box(model);
    if local_x < map.x || local_y < map.y {
        return None;
    }
    let column = (local_x - map.x) / map.size;
    let row = (local_y - map.y) / map.size;
    if column < 0 || row < 0 || row >= BOARD_HEIGHT as i32 {
        return None;
    }
    let board_x = map.min as i32 + column;
    if board_x > map.max as i32 {
        return None;
    }
    Some((row as usize * BOARD_WIDTH + board_x as usize) as u8)
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(cell) = local_cell(world, entity, *x, *y) else {
        return false;
    };
    let Some(position) = world.resource::<EchoModel>().map(EchoModel::position) else {
        return false;
    };
    let dx = i16::from(cell % BOARD_WIDTH as u8) - i16::from(position % BOARD_WIDTH as u8);
    let dy = i16::from(cell / BOARD_WIDTH as u8) - i16::from(position / BOARD_WIDTH as u8);
    let direction = match (dx, dy) {
        (0, -1) => Some(Direction::Up),
        (1, 0) => Some(Direction::Right),
        (0, 1) => Some(Direction::Down),
        (-1, 0) => Some(Direction::Left),
        _ => None,
    };
    if let Some(direction) = direction {
        EchoNodes::dispatch(world, EchoCommand::Step(direction));
    }
    true
}

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

struct EchoKeyboardPlugin;

fn handle_key(world: &mut World, ch: char) -> bool {
    match ch {
        'w' | 'W' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Up)),
        'a' | 'A' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Left)),
        's' | 'S' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Down)),
        'd' | 'D' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Right)),
        ' ' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Wait)),
        'r' | 'R' => EchoNodes::dispatch(world, EchoCommand::Rewind),
        'u' | 'U' => EchoNodes::dispatch(world, EchoCommand::Undo),
        _ => return false,
    }
    true
}

impl<B, F> Plugin<B, F> for EchoKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        handle_key(world, *ch)
    }
}

#[compose]
fn build_widgets() {
    ui! {
        EchoSurface (id: "echo_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "ECHO WALKER",
                position: Position::Absolute,
                left: 14,
                top: 7,
                width: 120,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "ECHO WALKER / 002718",
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
                "ARCHIVE 01 / 12",
                id: "echo_room",
                position: Position::Absolute,
                left: 14,
                top: 39,
                width: 110,
                height: 16,
                font_size: 9,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "MEMORY 0/2",
                id: "echo_memory",
                position: Position::Absolute,
                left: 146,
                top: 39,
                width: 90,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "ECHOES 0/3",
                id: "echo_ghosts",
                position: Position::Absolute,
                left: 220,
                top: 39,
                width: 70,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "THE PRESENT",
                position: Position::Absolute,
                left: 305,
                top: 66,
                width: 150,
                height: 12,
                font_size: 8,
                text_color: ACCENT,
                paragraph: label()
            )
            Text (
                "00",
                id: "echo_tick",
                position: Position::Absolute,
                left: 305,
                top: 82,
                width: 48,
                height: 38,
                font_size: 29,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                "/ 48 BEATS",
                position: Position::Absolute,
                left: 351,
                top: 99,
                width: 90,
                height: 14,
                font_size: 9,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "PAST MOVES ON EVERY BEAT",
                position: Position::Absolute,
                left: 305,
                top: 119,
                width: 161,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: label()
            )
            Button (
                "↑",
                position: Position::Absolute,
                left: 354,
                top: 133,
                width: 29,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 15,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Up)); }
            Button (
                "←",
                position: Position::Absolute,
                left: 321,
                top: 166,
                width: 29,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 15,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Left)); }
            Button (
                "·",
                position: Position::Absolute,
                left: 354,
                top: 166,
                width: 29,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 14,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Wait)); }
            Button (
                "→",
                position: Position::Absolute,
                left: 387,
                top: 166,
                width: 29,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 15,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Right)); }
            Button (
                "↓",
                position: Position::Absolute,
                left: 354,
                top: 199,
                width: 29,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 15,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Down)); }
            Button (
                "RECORD ECHO",
                id: "echo_rewind",
                position: Position::Absolute,
                left: 305,
                top: 236,
                width: 161,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: RUST,
                text_color: BACKGROUND,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Rewind); }
            Button (
                "UNDO",
                id: "echo_undo",
                position: Position::Absolute,
                left: 305,
                top: 270,
                width: 48,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Undo); }
            Button (
                "RETRY",
                id: "echo_restart",
                position: Position::Absolute,
                left: 360,
                top: 270,
                width: 49,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Restart); }
            Button (
                "TAPES",
                id: "echo_tapes",
                position: Position::Absolute,
                left: 416,
                top: 270,
                width: 50,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::update(ctx.world, |model| model.set_modal(EchoModal::Tapes)); }
            Text (
                "CURRENT ROUTE",
                position: Position::Absolute,
                left: 14,
                top: 281,
                width: 90,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                "0 STEPS / 0 ECHOES",
                id: "echo_run",
                position: Position::Absolute,
                left: 165,
                top: 281,
                width: 123,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "Reach a plate, rewind, and let the past replay the route.",
                id: "echo_message",
                position: Position::Absolute,
                left: 14,
                top: 301,
                width: 452,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: label()
            )
            EchoModalSurface (
                id: "echo_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) {
                Text (
                    "ECHO TAPES",
                    id: "echo_modal_title",
                    position: Position::Absolute,
                    left: 35,
                    top: 54,
                    width: 310,
                    height: 22,
                    font_size: 14,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    "Inspect each fixed route.",
                    id: "echo_modal_subtitle",
                    position: Position::Absolute,
                    left: 35,
                    top: 78,
                    width: 390,
                    height: 14,
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
                    font_size: 15,
                    normal_color: HEADER,
                    pressed_color: LINE,
                    text_color: TEXT,
                    border_radius: 4
                ) on Tap { EchoNodes::update(ctx.world, |model| model.set_modal(EchoModal::None)); }
                Button (
                    "ECHO 1",
                    id: "echo_tape_0",
                    position: Position::Absolute,
                    left: 35,
                    top: 111,
                    width: 412,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: HEADER,
                    pressed_color: LINE,
                    text_color: TEXT,
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(0) { None } else { Some(0) },
                                )
                        },
                    );
                }
                Button (
                    "ECHO 2",
                    id: "echo_tape_1",
                    position: Position::Absolute,
                    left: 35,
                    top: 150,
                    width: 412,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: HEADER,
                    pressed_color: LINE,
                    text_color: TEXT,
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(1) { None } else { Some(1) },
                                )
                        },
                    );
                }
                Button (
                    "ECHO 3",
                    id: "echo_tape_2",
                    position: Position::Absolute,
                    left: 35,
                    top: 189,
                    width: 412,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: HEADER,
                    pressed_color: LINE,
                    text_color: TEXT,
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(2) { None } else { Some(2) },
                                )
                        },
                    );
                }
                Text (
                    "ROOM 0 STEPS · 0 REWINDS",
                    id: "echo_result_stats",
                    position: Position::Absolute,
                    left: 35,
                    top: 126,
                    width: 380,
                    height: 30,
                    font_size: 20,
                    text_color: ACCENT,
                    paragraph: label()
                )
                Button (
                    "NEXT ROOM",
                    id: "echo_modal_primary",
                    position: Position::Absolute,
                    left: 35,
                    top: 236,
                    width: 255,
                    height: 36,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ACCENT,
                    pressed_color: RUST,
                    text_color: BACKGROUND,
                    border_radius: 4
                ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Continue); }
                Button (
                    "CLEAR ROOM",
                    id: "echo_modal_secondary",
                    position: Position::Absolute,
                    left: 303,
                    top: 236,
                    width: 144,
                    height: 36,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: HEADER,
                    pressed_color: LINE,
                    text_color: TEXT,
                    border_radius: 4
                ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Clear); }
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
    app.world.insert_resource(EchoModel::default());
    #[cfg(feature = "persistence")]
    app.world
        .insert_resource(EchoReplayLog::new(ReplayKind::Echo, 2718));
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_plugin(EchoKeyboardPlugin);
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Echo Walker node");
    let nodes = EchoNodes {
        surface: find("echo_surface"),
        room: find("echo_room"),
        memory: find("echo_memory"),
        ghosts: find("echo_ghosts"),
        tick: find("echo_tick"),
        run: find("echo_run"),
        message: find("echo_message"),
        actions: [
            find("echo_rewind"),
            find("echo_undo"),
            find("echo_restart"),
            find("echo_tapes"),
        ],
        modal: find("echo_modal"),
        modal_title: find("echo_modal_title"),
        modal_subtitle: find("echo_modal_subtitle"),
        modal_primary: find("echo_modal_primary"),
        modal_secondary: find("echo_modal_secondary"),
        tape_rows: [
            find("echo_tape_0"),
            find("echo_tape_1"),
            find("echo_tape_2"),
        ],
        result_stats: find("echo_result_stats"),
    };
    app.world.insert_resource(nodes);
    EchoNodes::sync(&mut app.world);
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_echo_walker.bin"))
        .bytes(
            "echo_walker/replay",
            |world| {
                world
                    .resource::<EchoReplayLog>()
                    .map(|log| log.encode_vec())
            },
            |world, bytes| {
                let Ok(log) = EchoReplayLog::decode(bytes, ReplayKind::Echo) else {
                    return;
                };
                let Ok(model) = replay_echo(&log) else {
                    return;
                };
                world.insert_resource(log);
                world.insert_resource(model);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
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
        assert!(app.world.find_by_id("echo_surface").is_some());
        assert!(app.world.find_by_id("echo_rewind").is_some());
        assert_eq!(app.world.query::<EchoSurface>().iter().count(), 1);
        assert!(app.world.query::<Button>().iter().count() >= 15);
    }

    #[test]
    fn board_hit_testing_maps_the_player_cell() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        app.render().unwrap();
        let surface = app.world.find_by_id("echo_surface").unwrap();
        let model = app.world.resource::<EchoModel>().unwrap();
        let map = map_box(model);
        let (x, y) = cell_origin(model.position(), map);
        let rect = app.world.get::<ComputedRect>(surface).unwrap().0;
        let screen_x = rect.x + Fixed::from_int(x + map.size / 2) * rect.w / Fixed::from_int(480);
        let screen_y = rect.y + Fixed::from_int(y + map.size / 2) * rect.h / Fixed::from_int(320);
        assert_eq!(
            local_cell(&app.world, surface, screen_x, screen_y),
            Some(model.position())
        );
    }

    #[test]
    fn keyboard_wait_advances_exactly_one_beat() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(handle_key(&mut app.world, ' '));
        assert_eq!(app.world.resource::<EchoModel>().unwrap().tick(), 1);
        #[cfg(feature = "persistence")]
        assert_eq!(app.world.resource::<EchoReplayLog>().unwrap().len(), 1);
    }
}
