use alloc::format;

use super::style::{ACCENT, BACKGROUND, CONTROL, SUCCESS, TEXT};
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::lumen::{LEVEL_COUNT, LumenModel};
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[derive(Clone, Copy)]
pub(super) struct LumenNodes {
    pub(super) board: Entity,
    pub(super) level_name: Entity,
    pub(super) level_subtitle: Entity,
    pub(super) puzzle: Entity,
    pub(super) status: Entity,
    pub(super) status_subtitle: Entity,
    pub(super) moves: Entity,
    pub(super) completed: Entity,
    pub(super) hint_note: Entity,
    pub(super) undo: Entity,
    pub(super) scan: Entity,
    pub(super) next: Entity,
    pub(super) modal: Entity,
    pub(super) mirror_labels: [Entity; 7],
    pub(super) level_checks: [Entity; LEVEL_COUNT],
}

impl LumenNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut LumenModel) -> ChangeSet) {
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

    pub(super) fn sync(world: &mut World) {
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

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn lumen_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    LumenNodes::update(world, |model| model.advance_ms(elapsed));
}
