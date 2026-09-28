use alloc::format;

use super::style::{ACCENT, BACKGROUND, INK, LINE, MUTED};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::factory::{
    FactoryError, FactoryModal, FactoryModel, FactoryPage, FactoryStatus, FactoryTool, GRID_WIDTH,
    MISSIONS, ModuleKind,
};
use crate::prelude::*;
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct FactorySurface;

#[crate::component]
#[derive(Default)]
pub(super) struct FactoryModalSurface;

#[derive(Clone, Copy)]
pub(super) struct FactoryNodes {
    pub(super) surface: Entity,
    pub(super) order_meta: Entity,
    pub(super) power_meta: Entity,
    pub(super) tick_meta: Entity,
    pub(super) tabs: [Entity; 3],
    pub(super) pages: [Entity; 3],
    pub(super) line_values: [Entity; 7],
    pub(super) order_buttons: [Entity; 3],
    pub(super) order_descriptions: [Entity; 3],
    pub(super) telemetry_values: [Entity; 5],
    pub(super) footer: [Entity; 5],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) modal_buttons: [Entity; 6],
}

impl FactoryNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut FactoryModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<FactoryModel>()
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

    pub(super) fn result(
        world: &mut World,
        update: impl FnOnce(&mut FactoryModel) -> Result<ChangeSet, FactoryError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    pub(super) fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<FactoryModel>() else {
            return;
        };
        let page = model.page();
        let modal = model.modal();
        let tool = model.tool();
        let mission_index = model.mission_index();
        let mission = model.mission();
        let power = model.power();
        let cost = model.cost();
        let selected = model.cell(model.selected());
        let selected_index = model.selected();
        let status = model.status();
        let running = model.running();
        let history_len = model.history_len();
        let values = [
            model.tick(),
            model.produced(),
            model.delivered(),
            model.rejected(),
            u16::from(model.wip()),
        ];
        let line_values = [
            format!("{} / {}", model.delivered(), mission.goal),
            format!("{} / {}", cost, mission.budget),
            selected.map_or("待建造空位".into(), |cell| cell.kind.label().into()),
            format!(
                "C{} · R{}",
                selected_index % GRID_WIDTH + 1,
                selected_index / GRID_WIDTH + 1
            ),
            status_line(status, model.wip(), model.blocked(), model.rejected()),
            format!("方向 {}", direction_glyph(model.tool_direction())),
            tool_label(model.tool()).into(),
        ];
        let _ = model;

        set_text(
            world,
            nodes.order_meta,
            format!(
                "ORDER 0{} / {}:{}",
                mission_index + 1,
                values[2],
                mission.goal
            ),
        );
        set_text(
            world,
            nodes.power_meta,
            format!("PWR {power}/{}", mission.power),
        );
        set_text(world, nodes.tick_meta, format!("T+{:03}", values[0]));
        for (index, entity) in nodes.tabs.into_iter().enumerate() {
            set_button_state(world, entity, index == page_index(page), true);
        }
        for (index, entity) in nodes.pages.into_iter().enumerate() {
            set_hidden(world, entity, index != page_index(page));
        }
        for (entity, value) in nodes.line_values.into_iter().zip(line_values) {
            set_text(world, entity, value);
        }
        for (index, entity) in nodes.order_buttons.into_iter().enumerate() {
            set_text(
                world,
                entity,
                format!("0{}  {}", index + 1, MISSIONS[index].name),
            );
            set_button_state(world, entity, index == usize::from(mission_index), true);
        }
        for (index, entity) in nodes.order_descriptions.into_iter().enumerate() {
            set_text(
                world,
                entity,
                format!(
                    "{} 件 / 预算 {}\n{}",
                    MISSIONS[index].goal, MISSIONS[index].budget, MISSIONS[index].description
                ),
            );
        }
        for (entity, value) in nodes.telemetry_values.into_iter().zip(values) {
            set_text(world, entity, format!("{value}"));
        }

        let footer_labels = [
            "＋ 建造",
            "旋转 ↻",
            "撤销",
            "单步",
            if running { "Ⅱ 暂停" } else { "▶ 运行" },
        ];
        for (index, entity) in nodes.footer.into_iter().enumerate() {
            set_text(world, entity, footer_labels[index]);
            let enabled = match index {
                1 => {
                    selected.is_some_and(|cell| {
                        !matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock)
                    }) || tool != FactoryTool::Select
                }
                2 => history_len > 0,
                3 | 4 => power <= mission.power,
                _ => true,
            };
            set_button_state(
                world,
                entity,
                index == 4 || (index == 0 && modal == FactoryModal::Tools),
                enabled,
            );
        }

        set_hidden(world, nodes.modal, modal == FactoryModal::None);
        let (title, subtitle) = match modal {
            FactoryModal::None => ("", ""),
            FactoryModal::Tools => ("建造模块", "选择工具后点击格子；修改布局会重置试运行。"),
            FactoryModal::Confirm {
                reference: true, ..
            } => ("装载示范布局？", "当前产线、进度与撤销记录将被替换。"),
            FactoryModal::Confirm {
                reference: false, ..
            } => ("切换订单？", "当前产线、进度与撤销记录将被替换。"),
            FactoryModal::Help => (
                "模块工厂 / 操作手册",
                "矿石经熔炼和装配后交付；质检订单还需要通过质检台。",
            ),
        };
        set_text(world, nodes.modal_title, title);
        set_text(world, nodes.modal_subtitle, subtitle);
        for (index, entity) in nodes.modal_buttons.into_iter().enumerate() {
            let (label, visible, active) = match modal {
                FactoryModal::Tools => match index {
                    0 => ("选择 / 检查", true, matches!(tool, FactoryTool::Select)),
                    1 => (
                        "传送带 · 1 信用",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Belt)),
                    ),
                    2 => (
                        "熔炼炉 · 4 / 3P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Furnace)),
                    ),
                    3 => (
                        "装配机 · 5 / 4P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Assembler)),
                    ),
                    4 => (
                        "质检台 · 3 / 2P",
                        true,
                        matches!(tool, FactoryTool::Build(ModuleKind::Inspector)),
                    ),
                    _ => ("拆除模块", true, matches!(tool, FactoryTool::Erase)),
                },
                FactoryModal::Confirm { .. } => match index {
                    0 => ("取消", true, false),
                    1 => ("确认装载", true, true),
                    _ => ("", false, false),
                },
                FactoryModal::Help => (
                    if index == 0 { "明白了" } else { "" },
                    index == 0,
                    index == 0,
                ),
                FactoryModal::None => ("", false, false),
            };
            set_hidden(world, entity, !visible);
            if visible {
                set_text(world, entity, label);
                set_button_state(world, entity, active, true);
            }
        }
    }
}

fn page_index(page: FactoryPage) -> usize {
    match page {
        FactoryPage::Line => 0,
        FactoryPage::Orders => 1,
        FactoryPage::Telemetry => 2,
    }
}

fn direction_glyph(direction: u8) -> &'static str {
    match direction % 4 {
        0 => "→",
        1 => "↓",
        2 => "←",
        _ => "↑",
    }
}

fn tool_label(tool: FactoryTool) -> &'static str {
    match tool {
        FactoryTool::Select => "选择建造模块",
        FactoryTool::Build(kind) => kind.label(),
        FactoryTool::Erase => "拆除模块",
    }
}

fn status_line(
    status: FactoryStatus,
    wip: u8,
    blocked: u8,
    rejected: u16,
) -> alloc::string::String {
    match status {
        FactoryStatus::Won => "✓ 订单完成！可在订单页选择下一条产线。".into(),
        FactoryStatus::Timeout => "试运行到期；请修改布局后重试。".into(),
        FactoryStatus::Running => format!("生产中 / 在制 {wip} / 堵塞 {blocked} / 退回 {rejected}"),
        FactoryStatus::Ready | FactoryStatus::Paused => {
            format!("已暂停 / 在制 {wip} / 堵塞 {blocked} / 退回 {rejected}")
        }
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
            ACCENT.into()
        } else if enabled {
            INK.into()
        } else {
            LINE.into()
        };
        button.pressed_color = ACCENT.into();
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
