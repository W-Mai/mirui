use alloc::{format, string::String};

use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiState,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::picture::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, PictureCell, PictureMessage, PictureModel, PictureTool,
};
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

const BG: Color = Color::rgb(11, 18, 29);
const PANEL: Color = Color::rgb(18, 31, 45);
const CELL: Color = Color::rgb(35, 54, 68);
const GRID: Color = Color::rgb(63, 87, 99);
const MINT: Color = Color::rgb(99, 231, 198);
const LAVENDER: Color = Color::rgb(191, 169, 242);
const APRICOT: Color = Color::rgb(244, 185, 105);
const TEXT: Color = Color::rgb(233, 241, 237);
const MUTED: Color = Color::rgb(129, 154, 166);
const BOARD_AREA_X: i32 = 91;
const BOARD_AREA_WIDTH: i32 = 172;
const BOARD_AREA_HEIGHT: i32 = 191;
const ROW_IDS: [&str; 10] = [
    "pic_row_0",
    "pic_row_1",
    "pic_row_2",
    "pic_row_3",
    "pic_row_4",
    "pic_row_5",
    "pic_row_6",
    "pic_row_7",
    "pic_row_8",
    "pic_row_9",
];
const COLUMN_IDS: [&str; 10] = [
    "pic_col_0",
    "pic_col_1",
    "pic_col_2",
    "pic_col_3",
    "pic_col_4",
    "pic_col_5",
    "pic_col_6",
    "pic_col_7",
    "pic_col_8",
    "pic_col_9",
];
const MAP_CHAPTER_IDS: [&str; 6] = [
    "picture_map_chapter_0",
    "picture_map_chapter_1",
    "picture_map_chapter_2",
    "picture_map_chapter_3",
    "picture_map_chapter_4",
    "picture_map_chapter_5",
];
const MAP_LEVEL_IDS: [&str; 6] = [
    "picture_map_level_0",
    "picture_map_level_1",
    "picture_map_level_2",
    "picture_map_level_3",
    "picture_map_level_4",
    "picture_map_level_5",
];
const SUMMARY_LINE_IDS: [&str; 6] = [
    "picture_summary_line_0",
    "picture_summary_line_1",
    "picture_summary_line_2",
    "picture_summary_line_3",
    "picture_summary_line_4",
    "picture_summary_line_5",
];

#[derive(crate::Component, Default)]
struct PictureSurface;

#[derive(Clone, Copy)]
struct PictureNodes {
    surface: Entity,
    level: Entity,
    chapter: Entity,
    status: Entity,
    progress: Entity,
    next: Entity,
    fill: Entity,
    mark: Entity,
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
    row_clues: [Entity; 10],
    column_clues: [Entity; 10],
}

impl PictureNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut PictureModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<PictureModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    fn next(world: &mut World) {
        let (modal, level) = world
            .resource::<PictureModel>()
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
                Self::update(world, PictureModel::continue_campaign);
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
            .resource::<PictureModel>()
            .map(PictureModel::level_index)
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
                .resource::<PictureModel>()
                .is_some_and(|model| !model.record(level).completed());
        let changes = world
            .resource_mut::<PictureModel>()
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
        let Some(model) = world.resource::<PictureModel>().copied() else {
            return;
        };
        let index = model.level_index();
        let level = model.level();
        let map = world
            .resource::<ExpeditionUiState>()
            .copied()
            .unwrap_or_default();
        let status = match model.message() {
            PictureMessage::Ready => "读出行列线索，修复失落图谱".into(),
            PictureMessage::Observed => "观测点已锁定，不能覆盖".into(),
            PictureMessage::Undone => "已撤回上一笔".into(),
            PictureMessage::Hint(cell) => format!("提示已校准格点 {}", cell + 1),
            PictureMessage::Checked(0) => "当前标记没有冲突".into(),
            PictureMessage::Checked(errors) => format!("发现 {errors} 个冲突标记"),
            PictureMessage::Complete => "图谱修复完成".into(),
        };
        let next = match model.modal() {
            ExpeditionModal::Result => "下一幅",
            ExpeditionModal::Final => "重新观测",
            _ if index < model.unlocked() => "换一幅",
            _ => "第一幅",
        };
        for (entity, content) in [
            (nodes.level, format!("A{:02} / 36", index + 1)),
            (nodes.chapter, CHAPTER_NAMES[usize::from(index / 6)].into()),
            (nodes.status, status),
            (
                nodes.progress,
                format!(
                    "{}×{}   UNDO {:02}   HINT {}",
                    level.size(),
                    level.size(),
                    model.history_len(),
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
        for line in 0..10_u8 {
            let mut clues = [0; 5];
            let active = line < level.size();
            let row = if active {
                let len = model.clues(true, line, &mut clues);
                clue_string(&clues[..len], " ")
            } else {
                String::new()
            };
            if let Some(text) = world.get_mut::<Text>(nodes.row_clues[usize::from(line)]) {
                text.set_content(row);
            }
            let column = if active {
                let len = model.clues(false, line, &mut clues);
                clue_string(&clues[..len], "\n")
            } else {
                String::new()
            };
            if let Some(text) = world.get_mut::<Text>(nodes.column_clues[usize::from(line)]) {
                text.set_content(column);
            }
            let geometry = board_geometry(&model);
            if let Some(style) = world.get_mut::<Style>(nodes.row_clues[usize::from(line)]) {
                style.layout.left = Dimension::px(20);
                style.layout.width = Dimension::px(geometry.x - 28);
                style.layout.top = Dimension::px(geometry.y + i32::from(line) * geometry.cell);
                style.layout.height = Dimension::px(geometry.cell);
            }
            if let Some(style) = world.get_mut::<Style>(nodes.column_clues[usize::from(line)]) {
                style.layout.left = Dimension::px(geometry.x + i32::from(line) * geometry.cell);
                style.layout.top = Dimension::px(43);
                style.layout.width = Dimension::px(geometry.cell);
                style.layout.height = Dimension::px((geometry.y - 45).max(12));
            }
            world.invalidate(nodes.row_clues[usize::from(line)]);
            world.invalidate(nodes.column_clues[usize::from(line)]);
        }
        set_tool_state(world, nodes.fill, model.tool() == PictureTool::Fill, MINT);
        set_tool_state(
            world,
            nodes.mark,
            model.tool() == PictureTool::Mark,
            LAVENDER,
        );
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
            "图谱已复原"
        };
        for (entity, content) in [
            (nodes.result_title, result_title.into()),
            (
                nodes.result_stats,
                format!(
                    "星级 {} / 3   提示 {} 次   图纸 {}×{}",
                    model.record(index).stars(),
                    model.hints(),
                    level.size(),
                    level.size()
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

fn sync_map(world: &mut World, nodes: PictureNodes, model: PictureModel, map: ExpeditionUiState) {
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
            APRICOT,
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
        set_map_button(world, entity, level == model.level_index(), locked, APRICOT);
    }
}

fn set_map_button(world: &mut World, entity: Entity, active: bool, locked: bool, accent: Color) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            accent.into()
        } else if locked {
            Color::rgb(22, 35, 48).into()
        } else {
            CELL.into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BG.into()
        } else if locked {
            Color::rgb(74, 96, 109).into()
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

#[derive(Clone, Copy)]
struct BoardGeometry {
    x: i32,
    y: i32,
    cell: i32,
    size: i32,
}

fn board_geometry(model: &PictureModel) -> BoardGeometry {
    let size = model.level().size();
    let mut max_column_clues = 1;
    for column in 0..size {
        let mut clues = [0; 5];
        max_column_clues = max_column_clues.max(model.clues(false, column, &mut clues));
    }
    let cell = 26.min(
        (BOARD_AREA_HEIGHT - i32::try_from(max_column_clues).unwrap_or(1) * 10) / i32::from(size),
    );
    let board = cell * i32::from(size);
    BoardGeometry {
        x: BOARD_AREA_X + (BOARD_AREA_WIDTH - board) / 2,
        y: 61 + i32::try_from(max_column_clues).unwrap_or(1) * 10,
        cell,
        size: board,
    }
}

fn clue_string(values: &[u8], separator: &str) -> String {
    let mut output = String::new();
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push_str(separator);
        }
        if *value == 10 {
            output.push_str("10");
        } else {
            output.push(char::from(b'0' + *value));
        }
    }
    if output.is_empty() {
        output.push('0');
    }
    output
}

fn set_tool_state(world: &mut World, entity: Entity, active: bool, accent: Color) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active { accent.into() } else { CELL.into() };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active { BG.into() } else { TEXT.into() };
    }
    world.invalidate_visual(entity);
}

fn paint_board(painter: &mut PlayPainter<'_, '_>, model: &PictureModel) {
    let level = model.level();
    let size = level.size();
    let geometry = board_geometry(model);
    let cell_size = geometry.cell;
    painter.fill(
        Rect::new(
            geometry.x - 6,
            geometry.y - 6,
            geometry.size + 12,
            geometry.size + 12,
        ),
        PANEL,
        Fixed::from_int(7),
    );
    painter.border(
        Rect::new(
            geometry.x - 6,
            geometry.y - 6,
            geometry.size + 12,
            geometry.size + 12,
        ),
        GRID,
        Fixed::ONE,
        Fixed::from_int(7),
    );
    for cell in 0..size * size {
        let x = geometry.x + i32::from(cell % size) * cell_size;
        let y = geometry.y + i32::from(cell / size) * cell_size;
        let inner = cell_size - 1;
        let value = model.cell(cell);
        let fill = match value {
            PictureCell::Unknown => CELL,
            PictureCell::Filled => MINT,
            PictureCell::EmptyMark => Color::rgb(26, 40, 53),
        };
        painter.fill(Rect::new(x, y, inner, inner), fill, Fixed::from_int(2));
        painter.border(
            Rect::new(x, y, inner, inner),
            GRID,
            Fixed::ONE,
            Fixed::from_int(2),
        );
        if value == PictureCell::EmptyMark {
            let inset = (cell_size / 3).max(3);
            painter.line(
                Point::new(x + inset, y + inset),
                Point::new(x + cell_size - inset, y + cell_size - inset),
                MUTED,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x + cell_size - inset, y + inset),
                Point::new(x + inset, y + cell_size - inset),
                MUTED,
                Fixed::ONE,
            );
        }
        if level.is_given(cell) {
            painter.border(
                Rect::new(x + 2, y + 2, cell_size - 5, cell_size - 5),
                APRICOT,
                Fixed::ONE,
                Fixed::from_int(2),
            );
        }
        if model.cursor() == cell {
            painter.border(
                Rect::new(x - 1, y - 1, cell_size + 1, cell_size + 1),
                LAVENDER,
                Fixed::from_int(2),
                Fixed::from_int(2),
            );
        }
    }
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<PictureModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_board(&mut painter, model);
    painter.fill(Rect::new(324, 53, 142, 202), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(324, 53, 142, 202),
        GRID,
        Fixed::ONE,
        Fixed::from_int(9),
    );
}

fn surface_view() -> View {
    View::new("PictureSurface", 60, surface_render).with_filter::<PictureSurface>()
}

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    let model = world.resource::<PictureModel>()?;
    let size = model.level().size();
    let geometry = board_geometry(model);
    if local_x < Fixed::from_int(geometry.x)
        || local_y < Fixed::from_int(geometry.y)
        || local_x >= Fixed::from_int(geometry.x + geometry.size)
        || local_y >= Fixed::from_int(geometry.y + geometry.size)
    {
        return None;
    }
    let column = (local_x.to_int() - geometry.x) / geometry.cell;
    let row = (local_y.to_int() - geometry.y) / geometry.cell;
    if column >= 0 && row >= 0 && column < i32::from(size) && row < i32::from(size) {
        Some((row as u8) * size + column as u8)
    } else {
        None
    }
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PictureNodes::update(world, |model| {
                model.begin_stroke(cell) | model.end_stroke(false)
            });
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PictureNodes::update(world, |model| model.begin_stroke(cell));
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return true;
            };
            PictureNodes::update(world, |model| model.continue_stroke(cell));
        }
        GestureEvent::DragEnd { .. } => {
            PictureNodes::update(world, |model| model.end_stroke(false));
        }
        GestureEvent::DragCancel { .. } => {
            PictureNodes::update(world, |model| model.end_stroke(true));
        }
        _ => return false,
    }
    true
}

struct PictureKeyboardPlugin;

impl<B, F> Plugin<B, F> for PictureKeyboardPlugin
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
            'w' | 'W' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Up)),
            'd' | 'D' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Right)),
            's' | 'S' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Down)),
            'a' | 'A' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Left)),
            ' ' => PictureNodes::update(world, |model| model.apply_cursor(None)),
            'x' | 'X' => {
                PictureNodes::update(world, |model| model.apply_cursor(Some(PictureTool::Mark)))
            }
            'z' | 'Z' => PictureNodes::update(world, PictureModel::undo),
            'h' | 'H' => PictureNodes::update(world, PictureModel::reveal_hint),
            _ => return false,
        }
        true
    }
}

#[compose]
fn build_widgets() {
    ui! {
        PictureSurface (id: "picture_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "ATLAS RESTORATION",
                position: Position::Absolute,
                left: 15,
                top: 7,
                width: 230,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "picture_chapter",
                position: Position::Absolute,
                left: 247,
                top: 10,
                width: 127,
                height: 16,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "picture_level",
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
                "",
                id: "pic_row_0",
                position: Position::Absolute,
                left: 25,
                top: 82,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_1",
                position: Position::Absolute,
                left: 25,
                top: 100,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_2",
                position: Position::Absolute,
                left: 25,
                top: 118,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_3",
                position: Position::Absolute,
                left: 25,
                top: 136,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_4",
                position: Position::Absolute,
                left: 25,
                top: 154,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_5",
                position: Position::Absolute,
                left: 25,
                top: 172,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_6",
                position: Position::Absolute,
                left: 25,
                top: 190,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_7",
                position: Position::Absolute,
                left: 25,
                top: 208,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_8",
                position: Position::Absolute,
                left: 25,
                top: 226,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_row_9",
                position: Position::Absolute,
                left: 25,
                top: 244,
                width: 91,
                height: 18,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "",
                id: "pic_col_0",
                position: Position::Absolute,
                left: 125,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_1",
                position: Position::Absolute,
                left: 143,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_2",
                position: Position::Absolute,
                left: 161,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_3",
                position: Position::Absolute,
                left: 179,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_4",
                position: Position::Absolute,
                left: 197,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_5",
                position: Position::Absolute,
                left: 215,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_6",
                position: Position::Absolute,
                left: 233,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_7",
                position: Position::Absolute,
                left: 251,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_8",
                position: Position::Absolute,
                left: 269,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "",
                id: "pic_col_9",
                position: Position::Absolute,
                left: 287,
                top: 42,
                width: 18,
                height: 38,
                font_size: 6,
                text_color: MUTED,
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            )
            Text (
                "OBSERVATION TOOLS",
                position: Position::Absolute,
                left: 335,
                top: 63,
                width: 119,
                height: 15,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "FILL",
                id: "picture_fill",
                position: Position::Absolute,
                left: 336,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: MINT,
                pressed_color: MINT,
                text_color: BG,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, |model| model.set_tool(PictureTool::Fill)); }
            Button (
                "MARK",
                id: "picture_mark",
                position: Position::Absolute,
                left: 398,
                top: 86,
                width: 56,
                height: 31,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, |model| model.set_tool(PictureTool::Mark)); }
            Button (
                "UNDO",
                position: Position::Absolute,
                left: 336,
                top: 126,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: LAVENDER,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::undo); }
            Button (
                "REVEAL ONE",
                position: Position::Absolute,
                left: 336,
                top: 161,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::reveal_hint); }
            Button (
                "CHECK",
                position: Position::Absolute,
                left: 336,
                top: 196,
                width: 118,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: CELL,
                pressed_color: MINT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::check); }
            Text (
                "",
                id: "picture_progress",
                position: Position::Absolute,
                left: 334,
                top: 232,
                width: 121,
                height: 14,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "picture_status",
                position: Position::Absolute,
                left: 18,
                top: 274,
                width: 126,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "CHAPTERS",
                position: Position::Absolute,
                left: 150,
                top: 269,
                width: 79,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::open_map(ctx.world); }
            Button (
                "RULES",
                position: Position::Absolute,
                left: 235,
                top: 269,
                width: 82,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::open_rules(ctx.world); }
            Button (
                "RESET",
                position: Position::Absolute,
                left: 324,
                top: 269,
                width: 61,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: CELL,
                pressed_color: APRICOT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PictureNodes::update(ctx.world, PictureModel::restart); }
            Button (
                "",
                id: "picture_next",
                position: Position::Absolute,
                left: 391,
                top: 269,
                width: 75,
                height: 35,
                size: ButtonSize::Compact,
                font_size: 7,
                normal_color: MINT,
                pressed_color: LAVENDER,
                text_color: BG,
                border_radius: 7
            ) on Tap { PictureNodes::next(ctx.world); }
            View (
                id: "picture_result",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
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
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "picture_result_title",
                        height: 28,
                        font_size: 15,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "",
                        id: "picture_result_stats",
                        height: 22,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "下一幅图已经开放；提示不会影响关卡解锁。",
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
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::update(ctx.world, PictureModel::restart); }
                        Button (
                            "查看航图",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::open_map(ctx.world); }
                        Button (
                            "继续远征 →",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 33,
                            font_size: 9,
                            normal_color: APRICOT,
                            pressed_color: LAVENDER,
                            text_color: BG,
                            border_radius: 7
                        ) on Tap { PictureNodes::next(ctx.world); }
                    }
                }
            }
            View (
                id: "picture_map",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
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
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::close_map(ctx.world); }
                    }
                    Text (
                        "",
                        id: "picture_map_summary",
                        height: 14,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 26, column_gap: 7) {
                        Button (
                            "01",
                            id: "picture_map_chapter_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            id: "picture_map_chapter_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            id: "picture_map_chapter_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            id: "picture_map_chapter_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            id: "picture_map_chapter_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            id: "picture_map_chapter_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 25,
                            font_size: 9,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::set_map_chapter(ctx.world, 5); }
                    }
                    Text (
                        "",
                        id: "picture_map_chapter_name",
                        height: 16,
                        font_size: 12,
                        text_color: APRICOT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_map_chapter_mechanic",
                        height: 14,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 58, column_gap: 7) {
                        Button (
                            "01",
                            id: "picture_map_level_0",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 0); }
                        Button (
                            "02",
                            id: "picture_map_level_1",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 1); }
                        Button (
                            "03",
                            id: "picture_map_level_2",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 2); }
                        Button (
                            "04",
                            id: "picture_map_level_3",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 3); }
                        Button (
                            "05",
                            id: "picture_map_level_4",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 4); }
                        Button (
                            "06",
                            id: "picture_map_level_5",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 57,
                            font_size: 10,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 7
                        ) on Tap { PictureNodes::select_map_level(ctx.world, 5); }
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
                id: "picture_rules",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
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
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 26, align: AlignItems::Center) {
                        Text (
                            "ATLAS RESTORATION · RULES",
                            grow: 1.0,
                            height: 22,
                            font_size: 12,
                            text_color: APRICOT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "×",
                            size: ButtonSize::Compact,
                            width: 28,
                            height: 25,
                            font_size: 12,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::close_map(ctx.world); }
                    }
                    Text (
                        "01  边缘数字表示这一行或列中连续填色段的长度。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "02  例如 2 1：填两格，至少空一格，再填一格。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "03  选择填色或标空，按住拖动；同一笔画可一次撤销。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "04  青点是固定观测点，不可修改；空格不必全部标叉。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "05  方向键选格，空格使用当前工具，X 键标空。",
                        height: 20,
                        font_size: 10,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
            View (
                id: "picture_briefing",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 244)
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
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "",
                        id: "picture_briefing_title",
                        height: 24,
                        font_size: 13,
                        text_color: APRICOT,
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
                        id: "picture_briefing_mechanic",
                        height: 20,
                        font_size: 11,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "尺寸增加时优先处理大数字与完整行。",
                        height: 17,
                        font_size: 9,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "段与段之间至少隔一个空格；青点观测不可修改。",
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
                        normal_color: APRICOT,
                        pressed_color: LAVENDER,
                        text_color: BG,
                        border_radius: 7
                    ) on Tap { PictureNodes::close_map(ctx.world); }
                }
            }
            View (
                id: "picture_summary",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                bg_color: Color::rgba(7, 13, 22, 248)
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
                    border_color: APRICOT,
                    border_width: 1,
                    border_radius: 9
                ) {
                    Text (
                        "远征档案 · 星图修复局",
                        height: 25,
                        font_size: 13,
                        text_color: APRICOT,
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
                        id: "picture_summary_line_0",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_1",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_2",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_3",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_4",
                        height: 17,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "",
                        id: "picture_summary_line_5",
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
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 0); }
                        Button (
                            "02",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 1); }
                        Button (
                            "03",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 2); }
                        Button (
                            "04",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 3); }
                        Button (
                            "05",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 4); }
                        Button (
                            "06",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 26,
                            font_size: 7,
                            normal_color: CELL,
                            pressed_color: APRICOT,
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { PictureNodes::open_map_chapter(ctx.world, 5); }
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
    app.add_plugin(PictureKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(PictureModel::default());
    app.world.insert_resource(ExpeditionUiState::default());
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Atlas Restoration node");
    app.world.insert_resource(PictureNodes {
        surface: find("picture_surface"),
        level: find("picture_level"),
        chapter: find("picture_chapter"),
        status: find("picture_status"),
        progress: find("picture_progress"),
        next: find("picture_next"),
        fill: find("picture_fill"),
        mark: find("picture_mark"),
        map: find("picture_map"),
        map_summary: find("picture_map_summary"),
        map_chapter_name: find("picture_map_chapter_name"),
        map_chapter_mechanic: find("picture_map_chapter_mechanic"),
        map_chapters: core::array::from_fn(|index| find(MAP_CHAPTER_IDS[index])),
        map_levels: core::array::from_fn(|index| find(MAP_LEVEL_IDS[index])),
        rules: find("picture_rules"),
        result: find("picture_result"),
        result_title: find("picture_result_title"),
        result_stats: find("picture_result_stats"),
        briefing: find("picture_briefing"),
        briefing_title: find("picture_briefing_title"),
        briefing_mechanic: find("picture_briefing_mechanic"),
        summary: find("picture_summary"),
        summary_lines: core::array::from_fn(|index| find(SUMMARY_LINE_IDS[index])),
        row_clues: core::array::from_fn(|index| find(ROW_IDS[index])),
        column_clues: core::array::from_fn(|index| find(COLUMN_IDS[index])),
    });
    PictureNodes::sync(&mut app.world);
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_atlas_restoration.bin"))
        .bytes(
            "atlas_restoration/save",
            |world| {
                world
                    .resource::<PictureModel>()
                    .map(PictureModel::encode_vec)
            },
            |world, bytes| {
                if let Ok(model) = PictureModel::decode(bytes) {
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
    fn composition_has_semantic_controls_and_clues() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("picture_surface").is_some());
        assert!(app.world.find_by_id("picture_fill").is_some());
        assert!(app.world.find_by_id("pic_col_9").is_some());
        let map = app.world.find_by_id("picture_map").unwrap();
        assert!(app.world.has::<Hidden>(map));
        let locked = app.world.find_by_id("picture_map_level_1").unwrap();
        assert!(!app.world.has::<HitTarget>(locked));
        PictureNodes::open_map(&mut app.world);
        assert!(!app.world.has::<Hidden>(map));
        PictureNodes::select_map_level(&mut app.world, 1);
        assert!(!app.world.has::<Hidden>(map));
        PictureNodes::select_map_level(&mut app.world, 0);
        assert!(app.world.has::<Hidden>(map));
        let rules = app.world.find_by_id("picture_rules").unwrap();
        PictureNodes::open_rules(&mut app.world);
        assert!(!app.world.has::<Hidden>(rules));
        PictureNodes::close_map(&mut app.world);
        assert!(app.world.has::<Hidden>(rules));
        for _ in 0..25 {
            app.world
                .resource_mut::<PictureModel>()
                .unwrap()
                .reveal_hint();
        }
        PictureNodes::sync(&mut app.world);
        let result = app.world.find_by_id("picture_result").unwrap();
        assert!(!app.world.has::<Hidden>(result));
        PictureNodes::update(&mut app.world, PictureModel::restart);
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_briefing();
        PictureNodes::sync(&mut app.world);
        let briefing = app.world.find_by_id("picture_briefing").unwrap();
        assert!(!app.world.has::<Hidden>(briefing));
        app.world
            .resource_mut::<ExpeditionUiState>()
            .unwrap()
            .open_summary();
        PictureNodes::sync(&mut app.world);
        let summary = app.world.find_by_id("picture_summary").unwrap();
        assert!(!app.world.has::<Hidden>(summary));
        assert_eq!(app.world.query::<PictureSurface>().iter().count(), 1);
    }

    #[test]
    fn first_picture_matches_reference_geometry() {
        let model = PictureModel::default();
        let geometry = board_geometry(&model);
        assert_eq!(
            (geometry.x, geometry.y, geometry.cell, geometry.size),
            (112, 81, 26, 130)
        );
    }
}
