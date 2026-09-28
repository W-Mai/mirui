use alloc::format;

use super::style::{ACTIVE, BACKGROUND, CONTROL, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::moss::{MossModal, MossModel, MossTool};
use crate::prelude::{Color, Entity, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct MossSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct MossModalSurface;

#[derive(Clone, Copy)]
pub(super) struct MossNodes {
    pub(super) surface: Entity,
    pub(super) status: Entity,
    pub(super) generation: Entity,
    pub(super) live: Entity,
    pub(super) rate: Entity,
    pub(super) undo: Entity,
    pub(super) run: Entity,
    pub(super) rotate: Entity,
    pub(super) tools: [Entity; 3],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) seed_buttons: [Entity; 3],
    pub(super) clear_controls: [Entity; 4],
}

impl MossNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut MossModel) -> ChangeSet) {
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

    pub(super) fn sync(world: &mut World) {
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
