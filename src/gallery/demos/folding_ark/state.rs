use alloc::format;

use super::style::{BG, FLOOR, LAVENDER, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiState,
};
use crate::gallery::play::fold::{
    CHAPTER_MECHANICS, CHAPTER_NAMES, FoldMessage, FoldModel, HullPose,
};
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct FoldSurface;

#[derive(Clone, Copy)]
pub(super) struct FoldNodes {
    pub(super) surface: Entity,
    pub(super) level: Entity,
    pub(super) chapter: Entity,
    pub(super) status: Entity,
    pub(super) steps: Entity,
    pub(super) next: Entity,
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
}

impl FoldNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut FoldModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<FoldModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes != ChangeSet::NONE {
            Self::sync(world);
        }
    }

    pub(super) fn hint(world: &mut World) {
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

    pub(super) fn next(world: &mut World) {
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

    pub(super) fn open_map(world: &mut World) {
        let level = world
            .resource::<FoldModel>()
            .map(FoldModel::level_index)
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

    pub(super) fn sync(world: &mut World) {
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
