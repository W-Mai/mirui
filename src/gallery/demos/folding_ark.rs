use alloc::format;

use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiState,
    FoldCell,
};
use crate::gallery::play::fold::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, FoldMessage, FoldModel, HullPose,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::scroll::TouchAction;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::surface::InputEvent;
use crate::ui::Hidden;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BG: Color = Color::rgb(9, 17, 28);
const PANEL: Color = Color::rgb(15, 29, 43);
const FLOOR: Color = Color::rgb(31, 53, 66);
const GRID: Color = Color::rgb(55, 83, 94);
const MINT: Color = Color::rgb(99, 231, 199);
const BLUE: Color = Color::rgb(112, 176, 239);
const LAVENDER: Color = Color::rgb(190, 169, 242);
const APRICOT: Color = Color::rgb(244, 184, 104);
const TEXT: Color = Color::rgb(232, 240, 236);
const MUTED: Color = Color::rgb(129, 153, 165);
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

#[derive(crate::Component, Default)]
struct FoldSurface;

#[derive(Clone, Copy)]
struct FoldNodes {
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

impl FoldNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut FoldModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<FoldModel>()
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
            .resource_mut::<FoldModel>()
            .map(|model| model.request_hint(&mut workspace))
            .unwrap_or(ChangeSet::NONE);
        world.insert_resource(workspace);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    fn next(world: &mut World) {
        let (modal, level) = world
            .resource::<FoldModel>()
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
                Self::update(world, FoldModel::continue_campaign);
                if level % 6 == 5 {
                    if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
                        state.open_briefing();
                    }
                    Self::sync(world);
                }
            }
            ExpeditionModal::None => Self::update(world, |model| {
                let next = if model.level_index() < model.unlocked() {
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
            .resource::<FoldModel>()
            .map(FoldModel::level_index)
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
                .resource::<FoldModel>()
                .is_some_and(|model| !model.record(level).completed());
        let changes = world
            .resource_mut::<FoldModel>()
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
        let Some(model) = world.resource::<FoldModel>().copied() else {
            return;
        };
        let map = world
            .resource::<ExpeditionUiState>()
            .copied()
            .unwrap_or_default();
        let index = model.level_index();
        let level = model.level();
        let status = match model.message() {
            FoldMessage::Ready => "翻滚船体，让它直立停靠终点".into(),
            FoldMessage::Unsupported => "船体失去支撑；这一步没有执行".into(),
            FoldMessage::Seal => "航标封印已收集".into(),
            FoldMessage::Bridge(true) => "潮桥已经升起".into(),
            FoldMessage::Bridge(false) => "潮桥已经收回".into(),
            FoldMessage::Undone => "已撤回上一段翻滚".into(),
            FoldMessage::Hint(direction, remaining) => {
                format!("提示 {} · 最短还需 {remaining} 步", direction.label())
            }
            FoldMessage::Complete => "船体已直立归港".into(),
        };
        let pose = match model.pose() {
            HullPose::Upright => "UPRIGHT",
            HullPose::Horizontal => "HORIZONTAL",
            HullPose::Vertical => "VERTICAL",
        };
        let next = match model.modal() {
            ExpeditionModal::Result => "下一海域",
            ExpeditionModal::Final => "再次启航",
            _ if index < model.unlocked() => "换一关",
            _ => "第一关",
        };
        for (entity, content) in [
            (nodes.level, format!("F{:02} / 36", index + 1)),
            (nodes.chapter, CHAPTER_NAMES[usize::from(index / 6)].into()),
            (nodes.status, status),
            (
                nodes.steps,
                format!(
                    "{pose}   STEP {:02}   PAR {:02}",
                    model.steps(),
                    level.par()
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
        } else if index % 6 == 5 {
            "章节完成"
        } else {
            "方舟已归港"
        };
        for (entity, content) in [
            (nodes.result_title, result_title.into()),
            (
                nodes.result_stats,
                format!(
                    "星级 {} / 3   完成 {:02} 步   最短 {:02} 步",
                    model.record(index).stars(),
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
                    index / 6 + 1,
                    CHAPTER_NAMES[usize::from(index / 6)]
                ),
            ),
            (
                nodes.briefing_mechanic,
                CHAPTER_MECHANICS[usize::from(index / 6)].into(),
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

fn sync_map(world: &mut World, nodes: FoldNodes, model: FoldModel, map: ExpeditionUiState) {
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
            LAVENDER.into()
        } else if locked {
            BG.into()
        } else {
            FLOOR.into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BG.into()
        } else if locked {
            Color::rgb(70, 91, 103).into()
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

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &FoldModel) {
    let level = model.level();
    painter.fill(Rect::new(13, 50, 292, 205), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(13, 50, 292, 205),
        Color::rgb(42, 68, 81),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for cell in 0..70_u8 {
        let x = 27 + i32::from(cell % 10) * 26;
        let y = 63 + i32::from(cell / 10) * 26;
        let kind = level.cell(cell);
        if kind == FoldCell::Void {
            continue;
        }
        let fill = match kind {
            FoldCell::Solid => FLOOR,
            FoldCell::Fragile => Color::rgb(62, 55, 76),
            FoldCell::Bridge if model.bridge_on() => BLUE,
            FoldCell::Bridge => Color::rgb(20, 35, 47),
            FoldCell::Switch => Color::rgb(45, 81, 74),
            FoldCell::Void => BG,
        };
        painter.fill(Rect::new(x, y, 23, 23), fill, Fixed::from_int(4));
        painter.border(
            Rect::new(x, y, 23, 23),
            GRID,
            Fixed::ONE,
            Fixed::from_int(4),
        );
        if level.goal() == cell {
            painter.border(
                Rect::new(x + 5, y + 5, 13, 13),
                MINT,
                Fixed::ONE,
                Fixed::from_int(3),
            );
        }
        for seal in 0..usize::from(level.seal_count()) {
            if level.seal(seal) == Some(cell) && model.seal_bits() & (1 << seal) == 0 {
                painter.circle(Point::new(x + 11, y + 11), Fixed::from_int(4), APRICOT);
            }
        }
        if kind == FoldCell::Switch {
            painter.circle(Point::new(x + 11, y + 11), Fixed::from_int(3), LAVENDER);
        }
    }
    let (occupied, len) = model.occupied();
    for cell in &occupied[..usize::from(len)] {
        let x = 27 + i32::from(*cell % 10) * 26;
        let y = 63 + i32::from(*cell / 10) * 26;
        painter.fill(Rect::new(x + 2, y + 2, 19, 19), MINT, Fixed::from_int(5));
        painter.border(
            Rect::new(x + 5, y + 5, 13, 13),
            BG,
            Fixed::ONE,
            Fixed::from_int(3),
        );
    }
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<FoldModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_map(&mut painter, model);
    painter.fill(Rect::new(317, 50, 149, 205), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(317, 50, 149, 205),
        Color::rgb(42, 68, 81),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    painter.circle(
        Point::new(337, 70),
        Fixed::from_int(5),
        if model.bridge_on() { BLUE } else { MUTED },
    );
    painter.circle(Point::new(355, 70), Fixed::from_int(5), APRICOT);
    painter.line(Point::new(329, 91), Point::new(454, 91), GRID, Fixed::ONE);
}

fn surface_view() -> View {
    View::new("FoldSurface", 60, surface_render).with_filter::<FoldSurface>()
}

fn move_model(world: &mut World, direction: Direction4) {
    FoldNodes::update(world, |model| model.move_direction(direction));
}

struct FoldKeyboardPlugin;

impl<B, F> Plugin<B, F> for FoldKeyboardPlugin
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
            'z' | 'Z' => FoldNodes::update(world, FoldModel::undo),
            'h' | 'H' => FoldNodes::hint(world),
            _ => return false,
        }
        true
    }
}

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

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
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

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_has_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("fold_surface").is_some());
        assert!(app.world.find_by_id("fold_next").is_some());
        let map = app.world.find_by_id("fold_map").unwrap();
        assert!(app.world.has::<Hidden>(map));
        let locked = app.world.find_by_id("fold_map_level_1").unwrap();
        assert!(!app.world.has::<HitTarget>(locked));
        FoldNodes::open_map(&mut app.world);
        assert!(!app.world.has::<Hidden>(map));
        FoldNodes::select_map_level(&mut app.world, 1);
        assert!(!app.world.has::<Hidden>(map));
        FoldNodes::select_map_level(&mut app.world, 0);
        assert!(app.world.has::<Hidden>(map));
        let rules = app.world.find_by_id("fold_rules").unwrap();
        FoldNodes::open_rules(&mut app.world);
        assert!(!app.world.has::<Hidden>(rules));
        FoldNodes::close_map(&mut app.world);
        assert!(app.world.has::<Hidden>(rules));
        let solution_len = app
            .world
            .resource::<FoldModel>()
            .unwrap()
            .level()
            .solution_len();
        for step in 0..solution_len {
            let direction = app
                .world
                .resource::<FoldModel>()
                .unwrap()
                .level()
                .solution(step)
                .unwrap();
            app.world
                .resource_mut::<FoldModel>()
                .unwrap()
                .move_direction(direction);
        }
        FoldNodes::sync(&mut app.world);
        let result = app.world.find_by_id("fold_result").unwrap();
        assert!(!app.world.has::<Hidden>(result));
        FoldNodes::update(&mut app.world, FoldModel::restart);
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_briefing();
        FoldNodes::sync(&mut app.world);
        let briefing = app.world.find_by_id("fold_briefing").unwrap();
        assert!(!app.world.has::<Hidden>(briefing));
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_summary();
        FoldNodes::sync(&mut app.world);
        let summary = app.world.find_by_id("fold_summary").unwrap();
        assert!(!app.world.has::<Hidden>(summary));
        assert_eq!(app.world.query::<FoldSurface>().iter().count(), 1);
    }
}
