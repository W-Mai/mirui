use alloc::format;

use super::style::{ACCENT, BACKGROUND, CONTROL, CONTROL_DISABLED, STATION_COLORS, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::post::{MAX_PARCELS, PostModal, PostModel};
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct PostSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct PostModalSurface;

#[derive(Clone, Copy)]
pub(super) struct PostNodes {
    pub(super) surface: Entity,
    pub(super) status: Entity,
    pub(super) sorted: Entity,
    pub(super) misses: Entity,
    pub(super) queue: [Entity; 5],
    pub(super) in_transit: Entity,
    pub(super) station_counts: [Entity; 3],
    pub(super) run: Entity,
    pub(super) send: Entity,
    pub(super) speed: Entity,
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) manifest_controls: [Entity; 3],
    pub(super) summary_controls: [Entity; 6],
    pub(super) reset_controls: [Entity; 4],
}

impl PostNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut PostModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<PostModel>()
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
        let Some(model) = world.resource::<PostModel>() else {
            return;
        };
        let modal = model.modal();
        let running = model.running();
        let started = model.started();
        let finished = model.finished();
        let manifest_id = model.manifest_id();
        let speed_x2 = model.speed_x2();
        let delivered = model.delivered();
        let missed = model.missed();
        let streak = model.streak();
        let score = model.score();
        let manifest_len = model.manifest_len();
        let active_len = model.active_len();
        let can_send = !finished
            && modal == PostModal::None
            && model.cursor() < manifest_len
            && usize::from(active_len) < MAX_PARCELS;
        let queued = core::array::from_fn::<_, 5, _>(|index| model.queued(index));
        let station_counts = core::array::from_fn::<_, 3, _>(|index| model.arrivals(index));
        let status = if finished {
            "班次完成"
        } else if running {
            "包裹正在路上"
        } else if started {
            "已暂停调度"
        } else {
            "准备好，拨动你的第一班轨道"
        };
        let run = if running {
            "暂停"
        } else if finished {
            "再开一班"
        } else if started {
            "继续"
        } else {
            "开始"
        };
        let speed = match speed_x2 {
            1 => "0.5× 速度",
            3 => "1.5× 速度",
            _ => "1× 速度",
        };
        let texts = [
            (nodes.status, status.into()),
            (nodes.sorted, format!("SORTED {delivered} / {manifest_len}")),
            (nodes.misses, format!("错投 {missed}   连对 {streak}")),
            (nodes.in_transit, format!("{active_len} / 3 在途")),
            (nodes.run, run.into()),
            (nodes.speed, speed.into()),
        ];
        let _ = model;
        for (entity, content) in texts {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for (index, entity) in nodes.queue.into_iter().enumerate() {
            let content = queued[index].map_or("", destination_letter);
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            if let Some(target) = queued[index]
                && let Some(style) = world.get_mut::<Style>(entity)
            {
                style.text_color = STATION_COLORS[usize::from(target)].into();
            }
            set_hidden(world, entity, queued[index].is_none());
        }
        for (index, entity) in nodes.station_counts.into_iter().enumerate() {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(format!("{}", station_counts[index]));
            }
            world.invalidate(entity);
        }
        set_button_state(
            world,
            nodes.run,
            running || !started,
            modal == PostModal::None,
        );
        set_button_state(world, nodes.send, false, can_send);
        set_button_state(world, nodes.speed, false, modal == PostModal::None);

        set_hidden(world, nodes.modal, modal == PostModal::None);
        let manifests_open = modal == PostModal::Manifests;
        let summary_open = modal == PostModal::Summary;
        let reset_open = modal == PostModal::Reset;
        for (index, entity) in nodes.manifest_controls.into_iter().enumerate() {
            set_hidden(world, entity, !manifests_open);
            set_button_state(world, entity, manifest_id as usize == index, manifests_open);
        }
        for entity in nodes.summary_controls {
            set_hidden(world, entity, !summary_open);
        }
        for entity in nodes.reset_controls {
            set_hidden(world, entity, !reset_open);
        }
        let (title, subtitle) = match modal {
            PostModal::Manifests => (
                "选择今天的班次",
                "更换班次会从头开始；没有时间惩罚，可以随时暂停。",
            ),
            PostModal::Summary => ("本班投递完成", "不需要抢时间，准确到达就很棒。"),
            PostModal::Reset => ("重新开始这一班？", "当前包裹、计分和道岔都会复位。"),
            PostModal::None => ("", ""),
        };
        if let Some(text) = world.get_mut::<Text>(nodes.modal_title) {
            text.set_content(title);
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_subtitle) {
            text.set_content(subtitle);
        }
        world.invalidate(nodes.modal_title);
        world.invalidate(nodes.modal_subtitle);

        if summary_open {
            let summary_texts = [
                (nodes.summary_controls[0], format!("{score:04}")),
                (nodes.summary_controls[2], format!("正确 {delivered} 件")),
                (nodes.summary_controls[3], format!("错投 {missed} 件")),
            ];
            for (entity, content) in summary_texts {
                if let Some(text) = world.get_mut::<Text>(entity) {
                    text.set_content(content);
                }
                world.invalidate(entity);
            }
        }
    }
}

fn destination_letter(destination: u8) -> &'static str {
    match destination {
        0 => "A",
        1 => "B",
        _ => "C",
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
            ACCENT.into()
        } else if enabled {
            CONTROL.into()
        } else {
            CONTROL_DISABLED.into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BACKGROUND.into()
        } else if enabled {
            TEXT.into()
        } else {
            Color::rgb(103, 120, 111).into()
        };
    }
    world.invalidate_visual(entity);
}
