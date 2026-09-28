use alloc::format;

use super::style::{BACKGROUND, INK, LINE, MUTED, ORANGE};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::orbit::{
    MAX_PREVIEW, MISSIONS, ManeuverNode, OrbitEventKind, OrbitModal, OrbitModel, OrbitPage,
    OrbitPoint, OrbitStatus,
};
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct OrbitSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct OrbitModalSurface;

#[derive(Clone, Copy)]
pub(super) struct OrbitNodes {
    pub(super) surface: Entity,
    pub(super) mission_meta: Entity,
    pub(super) time_meta: Entity,
    pub(super) tabs: [Entity; 3],
    pub(super) pages: [Entity; 3],
    pub(super) map_values: [Entity; 8],
    pub(super) plan_values: [Entity; 5],
    pub(super) record_values: [Entity; 6],
    pub(super) footer: [Entity; 5],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) modal_buttons: [Entity; 4],
}

#[derive(Clone, Copy)]
pub(super) struct OrbitPreview {
    pub(super) points: [OrbitPoint; MAX_PREVIEW],
    pub(super) len: u8,
    pub(super) sampled_at: crate::types::Fixed64,
    pub(super) body: crate::gallery::play::orbit::OrbitBody,
    pub(super) dv_tenths: u8,
    pub(super) angle: i16,
}

impl Default for OrbitPreview {
    fn default() -> Self {
        Self {
            points: [OrbitPoint::default(); MAX_PREVIEW],
            len: 0,
            sampled_at: crate::types::Fixed64::from_int(-1),
            body: Default::default(),
            dv_tenths: 0,
            angle: 0,
        }
    }
}

impl OrbitNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut OrbitModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<OrbitModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::MODEL) {
            Self::refresh_preview(world);
            Self::sync(world);
        } else if changes.contains(ChangeSet::VISUAL)
            && let Some(surface) = world.resource::<Self>().map(|nodes| nodes.surface)
        {
            world.invalidate_visual(surface);
        }
    }

    pub(super) fn result(
        world: &mut World,
        update: impl FnOnce(
            &mut OrbitModel,
        ) -> Result<ChangeSet, crate::gallery::play::orbit::OrbitError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    pub(super) fn refresh_preview(world: &mut World) {
        let Some(model) = world.resource::<OrbitModel>() else {
            return;
        };
        let body = model.body();
        let time = model.time();
        let dv_tenths = model.dv_tenths();
        let angle = model.angle_degrees();
        let refresh = world.resource::<OrbitPreview>().is_none_or(|preview| {
            preview.body != body
                && (time - preview.sampled_at).abs() >= crate::types::Fixed64::from_ratio(7, 10)
                || preview.dv_tenths != dv_tenths
                || preview.angle != angle
        });
        if !refresh {
            return;
        }
        let mut points = [OrbitPoint::default(); MAX_PREVIEW];
        let len = model.predict(&mut points);
        let preview = OrbitPreview {
            points,
            len: len as u8,
            sampled_at: time,
            body,
            dv_tenths,
            angle,
        };
        let _ = model;
        world.insert_resource(preview);
    }

    pub(super) fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<OrbitModel>() else {
            return;
        };
        let mission = model.mission();
        let page = model.page();
        let modal = model.modal();
        let status = model.status();
        let body = model.body();
        let map_values = [
            format!("FUEL  {} / 48", decimal(model.fuel(), 1)),
            format!("HEAT  {}%", decimal(model.heat(), 0)),
            format!("R  {}", decimal(body.radius(), 1)),
            format!("V  {}", decimal(body.speed(), 1)),
            format!("DV  {}.{}", model.dv_tenths() / 10, model.dv_tenths() % 10),
            format!("A  {}", model.angle_degrees()),
            if model.preview() {
                "预演 已开".into()
            } else {
                "预演 已关".into()
            },
            format!("{}×", model.warp()),
        ];
        let plan_values = [
            format!("{} s", model.delay_seconds()),
            node_text(model.queue(0), 1),
            node_text(model.queue(1), 2),
            node_text(model.queue(2), 3),
            format!("{} / 3 个节点", model.queue_len()),
        ];
        let latest = model
            .event_len()
            .checked_sub(1)
            .and_then(|index| model.event(index));
        let record_values = [
            format!("{}", model.telemetry_len()),
            format!("{}", model.trail_len()),
            decimal(body.radius(), 1),
            decimal(body.speed(), 1),
            latest.map_or("等待事件".into(), |event| {
                event_label(event.kind).into()
            }),
            status_label(status).into(),
        ];
        let footer_labels = [
            if status == OrbitStatus::Running {
                "Ⅱ 暂停"
            } else {
                "▶ 滑行"
            },
            "点火",
            if model.eligible() {
                "● 立即采样"
            } else {
                "采样"
            },
            "＋ 节点",
            "重新装载",
        ];
        let terminal = status.terminal();
        let _ = model;

        set_text(
            world,
            nodes.mission_meta,
            format!("任务 0{} · {}", mission_index(world) + 1, mission.name),
        );
        set_text(
            world,
            nodes.time_meta,
            format!("SIM T+{}", decimal_time(world)),
        );
        for (index, entity) in nodes.tabs.into_iter().enumerate() {
            set_button_state(world, entity, page_index(page) == index, true);
        }
        for (index, entity) in nodes.pages.into_iter().enumerate() {
            set_hidden(world, entity, page_index(page) != index);
        }
        for (entity, value) in nodes.map_values.into_iter().zip(map_values) {
            set_text(world, entity, value);
        }
        for (entity, value) in nodes.plan_values.into_iter().zip(plan_values) {
            set_text(world, entity, value);
        }
        for (entity, value) in nodes.record_values.into_iter().zip(record_values) {
            set_text(world, entity, value);
        }
        for (index, entity) in nodes.footer.into_iter().enumerate() {
            set_text(world, entity, footer_labels[index]);
            let enabled = match index {
                3 => !terminal && queue_len(world) < 3,
                _ => index == 4 || !terminal,
            };
            set_button_state(
                world,
                entity,
                index == 0 || (index == 2 && eligible(world)),
                enabled,
            );
        }
        set_hidden(world, nodes.modal, modal == OrbitModal::None);
        let (title, subtitle) = match modal {
            OrbitModal::None => ("", ""),
            OrbitModal::Missions => ("选择轨道任务", "切换会清空当前轨迹、计划与遥测记录。"),
            OrbitModal::Confirm(_) => ("重新装载任务？", "当前模拟状态将被重置。"),
            OrbitModal::Help => (
                "轨道任务台 / 操作手册",
                "设定机动并点火；进入目标窗口后手动采样。",
            ),
        };
        set_text(world, nodes.modal_title, title);
        set_text(world, nodes.modal_subtitle, subtitle);
        for (index, entity) in nodes.modal_buttons.into_iter().enumerate() {
            let (label, visible, active) = match modal {
                OrbitModal::Missions => (
                    MISSIONS.get(index).map_or("", |mission| mission.name),
                    index < 3,
                    index as u8 == mission_index(world),
                ),
                OrbitModal::Confirm(_) => match index {
                    0 => ("取消", true, false),
                    1 => ("确认装载", true, true),
                    _ => ("", false, false),
                },
                OrbitModal::Help => (
                    if index == 0 { "明白了" } else { "" },
                    index == 0,
                    index == 0,
                ),
                OrbitModal::None => ("", false, false),
            };
            set_hidden(world, entity, !visible);
            if visible {
                set_text(world, entity, label);
                set_button_state(world, entity, active, true);
            }
        }
        world.invalidate_visual(nodes.surface);
    }
}

fn decimal(value: crate::types::Fixed64, digits: u8) -> alloc::string::String {
    if digits == 0 {
        return format!("{}", value.to_int());
    }
    let scaled = (value * 10).to_int();
    format!("{}.{:01}", scaled / 10, scaled.unsigned_abs() % 10)
}

pub(super) fn mission_index(world: &World) -> u8 {
    world
        .resource::<OrbitModel>()
        .map_or(0, OrbitModel::mission_index)
}
fn decimal_time(world: &World) -> alloc::string::String {
    world
        .resource::<OrbitModel>()
        .map_or("0.0".into(), |model| decimal(model.time(), 1))
}
fn queue_len(world: &World) -> usize {
    world
        .resource::<OrbitModel>()
        .map_or(0, OrbitModel::queue_len)
}
fn eligible(world: &World) -> bool {
    world
        .resource::<OrbitModel>()
        .is_some_and(OrbitModel::eligible)
}

fn node_text(node: Option<ManeuverNode>, number: usize) -> alloc::string::String {
    node.map_or_else(
        || format!("0{number}  空节点"),
        |node| {
            format!(
                "0{number}  T+{}  Δv {}.{}  {}°",
                decimal(node.at, 1),
                node.dv_tenths / 10,
                node.dv_tenths % 10,
                node.angle_degrees
            )
        },
    )
}

fn event_label(kind: OrbitEventKind) -> &'static str {
    match kind {
        OrbitEventKind::Loaded => "任务装载 · 等待机动",
        OrbitEventKind::Burn => "机动点火完成",
        OrbitEventKind::NodeSkipped => "计划节点跳过",
        OrbitEventKind::Goal => "目标窗口采样完成",
        OrbitEventKind::Won => "全部目标完成",
        OrbitEventKind::Crashed => "接近中心边界 · 中止",
        OrbitEventKind::Escaped => "越出模拟区域 · 中止",
    }
}

fn status_label(status: OrbitStatus) -> &'static str {
    match status {
        OrbitStatus::Ready => "READY",
        OrbitStatus::Running => "RUNNING",
        OrbitStatus::Paused => "PAUSED",
        OrbitStatus::Won => "COMPLETE",
        OrbitStatus::Crashed => "CRASHED",
        OrbitStatus::Escaped => "ESCAPED",
    }
}

fn page_index(page: OrbitPage) -> usize {
    match page {
        OrbitPage::Map => 0,
        OrbitPage::Plan => 1,
        OrbitPage::Record => 2,
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<alloc::string::String>) {
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

fn set_button_state(world: &mut World, entity: Entity, active: bool, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            ORANGE.into()
        } else if enabled {
            INK.into()
        } else {
            LINE.into()
        };
        button.pressed_color = ORANGE.into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active || enabled {
            BACKGROUND.into()
        } else {
            MUTED.into()
        };
    }
    world.invalidate_visual(entity);
}
