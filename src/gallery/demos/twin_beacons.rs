use alloc::format;

use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiState,
    TwinCell,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::twin::{CHAPTER_MECHANICS, CHAPTER_NAMES, TwinMessage, TwinModel};
use crate::input::event::scroll::TouchAction;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::surface::InputEvent;
use crate::ui::Hidden;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BG: Color = Color::rgb(7, 18, 29);
const PANEL: Color = Color::rgb(12, 31, 43);
const FLOOR: Color = Color::rgb(24, 51, 64);
const WALL: Color = Color::rgb(7, 15, 24);
const GRID: Color = Color::rgb(44, 77, 88);
const MINT: Color = Color::rgb(102, 236, 208);
const LAVENDER: Color = Color::rgb(191, 172, 245);
const APRICOT: Color = Color::rgb(245, 190, 118);
const TEXT: Color = Color::rgb(231, 241, 238);
const MUTED: Color = Color::rgb(125, 154, 163);
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

#[derive(crate::Component, Default)]
struct TwinSurface;

#[derive(Clone, Copy)]
struct TwinNodes {
    surface: Entity,
    level: Entity,
    chapter: Entity,
    status: Entity,
    steps: Entity,
    next: Entity,
    map: Entity,
    map_summary: Entity,
    map_chapter_name: Entity,
    map_chapter_mechanic: Entity,
    map_chapters: [Entity; 6],
    map_levels: [Entity; 6],
    rules: Entity,
    result: Entity,
    result_title: Entity,
    result_stats: Entity,
    briefing: Entity,
    briefing_title: Entity,
    briefing_mechanic: Entity,
    summary: Entity,
    summary_lines: [Entity; 6],
}

impl TwinNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut TwinModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<TwinModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    fn hint(world: &mut World) {
        let mut workspace = world
            .remove_resource::<ExpeditionHintWorkspace>()
            .unwrap_or_default();
        let changes = world
            .resource_mut::<TwinModel>()
            .map(|model| model.request_hint(&mut workspace))
            .unwrap_or(ChangeSet::NONE);
        world.insert_resource(workspace);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    fn next(world: &mut World) {
        let (modal, level) = world
            .resource::<TwinModel>()
            .map(|model| (model.modal(), model.level_index()))
            .unwrap_or((ExpeditionModal::None, 0));
        match modal {
            ExpeditionModal::Final => {
                if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
                    state.open_summary();
                }
                Self::sync(world);
            }
            ExpeditionModal::Result => {
                Self::update(world, TwinModel::continue_campaign);
                if level % 6 == 5 {
                    if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
                        state.open_briefing();
                    }
                    Self::sync(world);
                }
            }
            ExpeditionModal::None => Self::update(world, |model| {
                let unlocked = model.unlocked();
                let next = if model.level_index() < unlocked {
                    model.level_index() + 1
                } else {
                    0
                };
                model.select_level(next)
            }),
        }
    }

    fn open_map(world: &mut World) {
        let level = world
            .resource::<TwinModel>()
            .map(TwinModel::level_index)
            .unwrap_or(0);
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open(level);
        }
        Self::sync(world);
    }

    fn close_map(world: &mut World) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.close();
        }
        Self::sync(world);
    }

    fn open_rules(world: &mut World) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open_rules();
        }
        Self::sync(world);
    }

    fn set_map_chapter(world: &mut World, chapter: u8) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.select_chapter(chapter);
        }
        Self::sync(world);
    }

    fn open_map_chapter(world: &mut World, chapter: u8) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open(chapter.saturating_mul(6));
        }
        Self::sync(world);
    }

    fn select_map_level(world: &mut World, slot: u8) {
        let chapter = world
            .resource::<ExpeditionUiState>()
            .map(|state| state.chapter())
            .unwrap_or(0);
        let level = chapter * 6 + slot;
        let briefing = level > 0
            && level % 6 == 0
            && world
                .resource::<TwinModel>()
                .is_some_and(|model| !model.record(level).completed());
        let changes = world
            .resource_mut::<TwinModel>()
            .map(|model| model.select_level(level))
            .unwrap_or(ChangeSet::NONE);
        if changes != ChangeSet::NONE
            && let Some(state) = world.resource_mut::<ExpeditionUiState>()
        {
            if briefing {
                state.open_briefing();
            } else {
                state.close();
            }
        }
        Self::sync(world);
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<TwinModel>().copied() else {
            return;
        };
        let map = world
            .resource::<ExpeditionUiState>()
            .copied()
            .unwrap_or_default();
        let level_index = model.level_index();
        let level = model.level();
        let status = match model.message() {
            TwinMessage::Ready => "双站同步，单次输入驱动两边".into(),
            TwinMessage::Blocked => "路径受阻；另一座信标也保持原位".into(),
            TwinMessage::KeyCollected => "访问密钥已同步，闸门开放".into(),
            TwinMessage::Undone => "已撤回上一条指令".into(),
            TwinMessage::Hint(direction, remaining) => {
                format!("提示 {} · 最短还需 {remaining} 步", direction.label())
            }
            TwinMessage::Complete => "两座信标已同时归位".into(),
        };
        let chapter = CHAPTER_NAMES[usize::from(level_index / 6)];
        let next = match model.modal() {
            ExpeditionModal::Result => "下一站",
            ExpeditionModal::Final => "再航行",
            _ if model.level_index() < model.unlocked() => "换一站",
            _ => "第一站",
        };
        for (entity, content) in [
            (nodes.level, format!("T{:02} / 36", level_index + 1)),
            (nodes.chapter, chapter.into()),
            (nodes.status, status),
            (
                nodes.steps,
                format!(
                    "STEP {:02}   PAR {:02}   HINT {}",
                    model.steps(),
                    level.par(),
                    model.hints()
                ),
            ),
            (nodes.next, next.into()),
        ] {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        sync_map(world, nodes, model, map);
        set_hidden(world, nodes.rules, map.panel() != ExpeditionPanel::Rules);
        set_hidden(
            world,
            nodes.briefing,
            map.panel() != ExpeditionPanel::Briefing,
        );
        set_hidden(
            world,
            nodes.summary,
            map.panel() != ExpeditionPanel::Summary,
        );
        set_hidden(world, nodes.result, model.modal() == ExpeditionModal::None);
        let result_title = if model.modal() == ExpeditionModal::Final {
            "远征完成"
        } else if level_index % 6 == 5 {
            "章节完成"
        } else {
            "两座信标已同步"
        };
        for (entity, content) in [
            (nodes.result_title, result_title.into()),
            (
                nodes.result_stats,
                format!(
                    "星级 {} / 3   完成 {:02} 步   最短 {:02} 步",
                    model.record(level_index).stars(),
                    model.steps(),
                    level.par()
                ),
            ),
        ] {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for (entity, content) in [
            (
                nodes.briefing_title,
                format!(
                    "第 {} 章 · {}",
                    level_index / 6 + 1,
                    CHAPTER_NAMES[usize::from(level_index / 6)]
                ),
            ),
            (
                nodes.briefing_mechanic,
                CHAPTER_MECHANICS[usize::from(level_index / 6)].into(),
            ),
        ] {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for chapter in 0..6_u8 {
            let mut completed = 0_u8;
            let mut stars = 0_u8;
            for slot in 0..6_u8 {
                let record = model.record(chapter * 6 + slot);
                if record.completed() {
                    completed += 1;
                    stars += record.stars();
                }
            }
            let entity = nodes.summary_lines[usize::from(chapter)];
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(format!(
                    "0{}  {}   {completed}/6   {stars}/18 ★",
                    chapter + 1,
                    CHAPTER_NAMES[usize::from(chapter)]
                ));
            }
            world.invalidate(entity);
        }
        world.invalidate_visual(nodes.surface);
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

fn sync_map(world: &mut World, nodes: TwinNodes, model: TwinModel, map: ExpeditionUiState) {
    set_hidden(world, nodes.map, map.panel() != ExpeditionPanel::Map);
    let completed = (0..36_u8)
        .filter(|level| model.record(*level).completed())
        .count();
    for (entity, content) in [
        (
            nodes.map_summary,
            format!(
                "COMPLETE {completed:02} / 36   CHAPTER {:02} / 06",
                map.chapter() + 1
            ),
        ),
        (
            nodes.map_chapter_name,
            CHAPTER_NAMES[usize::from(map.chapter())].into(),
        ),
        (
            nodes.map_chapter_mechanic,
            CHAPTER_MECHANICS[usize::from(map.chapter())].into(),
        ),
    ] {
        if let Some(text) = world.get_mut::<Text>(entity) {
            text.set_content(content);
        }
        world.invalidate(entity);
    }
    for chapter in 0..6_u8 {
        set_map_button(
            world,
            nodes.map_chapters[usize::from(chapter)],
            chapter == map.chapter(),
            false,
        );
    }
    for slot in 0..6_u8 {
        let level = map.chapter() * 6 + slot;
        let record = model.record(level);
        let locked = level > model.unlocked();
        let label = if locked {
            format!("{:02}\nLOCK", level + 1)
        } else if record.completed() {
            format!("{:02}\n{} STAR", level + 1, record.stars())
        } else if level == model.level_index() {
            format!("{:02}\nPLAY", level + 1)
        } else {
            format!("{:02}\nOPEN", level + 1)
        };
        let entity = nodes.map_levels[usize::from(slot)];
        if let Some(text) = world.get_mut::<Text>(entity) {
            text.set_content(label);
        }
        set_map_button(world, entity, level == model.level_index(), locked);
    }
}

fn set_map_button(world: &mut World, entity: Entity, active: bool, locked: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            MINT.into()
        } else if locked {
            WALL.into()
        } else {
            FLOOR.into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BG.into()
        } else if locked {
            Color::rgb(67, 91, 101).into()
        } else {
            TEXT.into()
        };
    }
    if locked {
        world.remove::<HitTarget>(entity);
        world.remove::<InteractionFeedback>(entity);
    } else {
        if !world.has::<HitTarget>(entity) {
            world.insert(entity, HitTarget);
        }
        if !world.has::<InteractionFeedback>(entity) {
            world.insert(entity, InteractionFeedback);
        }
    }
    world.invalidate_visual(entity);
}

fn paint_board(
    painter: &mut PlayPainter<'_, '_>,
    model: &TwinModel,
    station: usize,
    origin_x: i32,
    accent: Color,
) {
    let level = model.level();
    painter.fill(
        Rect::new(origin_x - 5, 54, 139, 151),
        PANEL,
        Fixed::from_int(8),
    );
    painter.border(
        Rect::new(origin_x - 5, 54, 139, 151),
        Color::rgb(34, 68, 80),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    for cell in 0..36_u8 {
        let x = origin_x + i32::from(cell % 6) * 21;
        let y = 67 + i32::from(cell / 6) * 21;
        let fill = match level.cell(station, cell) {
            TwinCell::Floor => FLOOR,
            TwinCell::Wall => WALL,
            TwinCell::Door => {
                if model.has_key() {
                    FLOOR
                } else {
                    APRICOT
                }
            }
        };
        painter.fill(Rect::new(x, y, 19, 19), fill, Fixed::from_int(3));
        painter.border(
            Rect::new(x, y, 19, 19),
            GRID,
            Fixed::ONE,
            Fixed::from_int(3),
        );
        if level.goal(station) == cell {
            painter.border(
                Rect::new(x + 4, y + 4, 11, 11),
                accent,
                Fixed::ONE,
                Fixed::from_int(5),
            );
        }
        if level.key() == Some(cell) && !model.has_key() {
            painter.circle(Point::new(x + 9, y + 9), Fixed::from_int(3), APRICOT);
        }
    }
    let position = model.position(station);
    let x = origin_x + i32::from(position % 6) * 21 + 9;
    let y = 67 + i32::from(position / 6) * 21 + 9;
    painter.circle(Point::new(x, y), Fixed::from_int(7), accent);
    painter.circle(Point::new(x, y), Fixed::from_int(3), BG);
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<TwinModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_board(&mut painter, model, 0, 18, MINT);
    paint_board(&mut painter, model, 1, 165, LAVENDER);
    painter.fill(Rect::new(316, 54, 150, 205), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(316, 54, 150, 205),
        Color::rgb(34, 68, 80),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    for y in [89, 128, 167, 206] {
        painter.line(Point::new(327, y), Point::new(455, y), GRID, Fixed::ONE);
    }
    painter.circle(Point::new(338, 73), Fixed::from_int(5), MINT);
    painter.circle(Point::new(355, 73), Fixed::from_int(5), LAVENDER);
}

fn surface_view() -> View {
    View::new("TwinSurface", 60, surface_render).with_filter::<TwinSurface>()
}

fn move_model(world: &mut World, direction: Direction4) {
    TwinNodes::update(world, |model| model.move_direction(direction));
}

struct TwinKeyboardPlugin;

impl<B, F> Plugin<B, F> for TwinKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        if world
            .resource::<ExpeditionUiState>()
            .is_some_and(|state| state.panel() != ExpeditionPanel::None)
        {
            return false;
        }
        match ch {
            'w' | 'W' => move_model(world, Direction4::Up),
            'd' | 'D' => move_model(world, Direction4::Right),
            's' | 'S' => move_model(world, Direction4::Down),
            'a' | 'A' => move_model(world, Direction4::Left),
            'z' | 'Z' => TwinNodes::update(world, TwinModel::undo),
            'h' | 'H' => TwinNodes::hint(world),
            _ => return false,
        }
        true
    }
}

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

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_has_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("twin_surface").is_some());
        assert!(app.world.find_by_id("twin_next").is_some());
        let map = app.world.find_by_id("twin_map").unwrap();
        assert!(app.world.has::<Hidden>(map));
        let locked = app.world.find_by_id("twin_map_level_1").unwrap();
        assert!(!app.world.has::<HitTarget>(locked));
        TwinNodes::open_map(&mut app.world);
        assert!(!app.world.has::<Hidden>(map));
        TwinNodes::select_map_level(&mut app.world, 1);
        assert!(!app.world.has::<Hidden>(map));
        TwinNodes::select_map_level(&mut app.world, 0);
        assert!(app.world.has::<Hidden>(map));
        let rules = app.world.find_by_id("twin_rules").unwrap();
        TwinNodes::open_rules(&mut app.world);
        assert!(!app.world.has::<Hidden>(rules));
        TwinNodes::close_map(&mut app.world);
        assert!(app.world.has::<Hidden>(rules));
        let solution_len = app
            .world
            .resource::<TwinModel>()
            .unwrap()
            .level()
            .solution_len();
        for step in 0..solution_len {
            let direction = app
                .world
                .resource::<TwinModel>()
                .unwrap()
                .level()
                .solution(step)
                .unwrap();
            app.world
                .resource_mut::<TwinModel>()
                .unwrap()
                .move_direction(direction);
        }
        TwinNodes::sync(&mut app.world);
        let result = app.world.find_by_id("twin_result").unwrap();
        assert!(!app.world.has::<Hidden>(result));
        TwinNodes::update(&mut app.world, TwinModel::restart);
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_briefing();
        TwinNodes::sync(&mut app.world);
        let briefing = app.world.find_by_id("twin_briefing").unwrap();
        assert!(!app.world.has::<Hidden>(briefing));
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_summary();
        TwinNodes::sync(&mut app.world);
        let summary = app.world.find_by_id("twin_summary").unwrap();
        assert!(!app.world.has::<Hidden>(summary));
        assert_eq!(app.world.query::<TwinSurface>().iter().count(), 1);
    }
}
