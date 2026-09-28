use alloc::{format, string::String};

use super::geometry::board_geometry;
use super::style::{APRICOT, BG, CELL, LAVENDER, MINT, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{ExpeditionModal, ExpeditionPanel, ExpeditionUiState};
use crate::gallery::play::picture::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, PictureMessage, PictureModel, PictureTool,
};
use crate::prelude::{Color, Dimension, Entity, HitTarget, InteractionFeedback, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct PictureSurface;

#[derive(Clone, Copy)]
pub(super) struct PictureNodes {
    pub(super) surface: Entity,
    pub(super) level: Entity,
    pub(super) chapter: Entity,
    pub(super) status: Entity,
    pub(super) progress: Entity,
    pub(super) next: Entity,
    pub(super) fill: Entity,
    pub(super) mark: Entity,
    pub(super) map: Entity,
    pub(super) map_summary: Entity,
    pub(super) map_chapter_name: Entity,
    pub(super) map_chapter_mechanic: Entity,
    pub(super) map_chapters: [Entity; 6],
    pub(super) map_levels: [Entity; 6],
    pub(super) rules: Entity,
    pub(super) result: Entity,
    pub(super) result_title: Entity,
    pub(super) result_stats: Entity,
    pub(super) briefing: Entity,
    pub(super) briefing_title: Entity,
    pub(super) briefing_mechanic: Entity,
    pub(super) summary: Entity,
    pub(super) summary_lines: [Entity; 6],
    pub(super) row_clues: [Entity; 10],
    pub(super) column_clues: [Entity; 10],
}

impl PictureNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut PictureModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<PictureModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    pub(super) fn next(world: &mut World) {
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

    pub(super) fn open_map(world: &mut World) {
        let level = world
            .resource::<PictureModel>()
            .map(PictureModel::level_index)
            .unwrap_or(0);
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open(level);
        }
        Self::sync(world);
    }

    pub(super) fn close_map(world: &mut World) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.close();
        }
        Self::sync(world);
    }

    pub(super) fn open_rules(world: &mut World) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open_rules();
        }
        Self::sync(world);
    }

    pub(super) fn set_map_chapter(world: &mut World, chapter: u8) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.select_chapter(chapter);
        }
        Self::sync(world);
    }

    pub(super) fn open_map_chapter(world: &mut World, chapter: u8) {
        if let Some(state) = world.resource_mut::<ExpeditionUiState>() {
            state.open(chapter.saturating_mul(6));
        }
        Self::sync(world);
    }

    pub(super) fn select_map_level(world: &mut World, slot: u8) {
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

    pub(super) fn sync(world: &mut World) {
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
