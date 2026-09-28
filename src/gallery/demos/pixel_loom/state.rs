use alloc::format;

use super::style::{ACTIVE, BACKGROUND, CONTROL, MUTED, PALETTE, TEMPLATE_NAMES, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::pixel::{PixelModal, PixelModel, PixelTool};
use crate::prelude::{Color, Entity, Fixed, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct PixelSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct PixelModalSurface;

#[derive(Clone, Copy)]
pub(super) struct PixelNodes {
    pub(super) surface: Entity,
    pub(super) frame_status: Entity,
    pub(super) mode_status: Entity,
    pub(super) template_name: Entity,
    pub(super) fps: Entity,
    pub(super) play: Entity,
    pub(super) undo: Entity,
    pub(super) frames: [Entity; 4],
    pub(super) tools: [Entity; 4],
    pub(super) colors: [Entity; 6],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) template_buttons: [Entity; 3],
    pub(super) clear_controls: [Entity; 3],
}

impl PixelNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut PixelModel) -> ChangeSet) {
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

    pub(super) fn sync(world: &mut World) {
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
