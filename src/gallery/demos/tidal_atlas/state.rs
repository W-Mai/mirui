use alloc::{format, string::String};

use super::style::{DARK, MUTED, PANEL, PAPER, TEXT};
use crate::gallery::play::change::ChangeSet;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::TidalReplayLog;
use crate::gallery::play::tidal::{
    Perk, TideCommand, TideLevel, TideMessage, TideModal, TideModel,
};
use crate::prelude::{Color, Entity, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct TideSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct TideModalSurface;

#[derive(Clone, Copy)]
pub(super) struct TideNodes {
    pub(super) surface: Entity,
    pub(super) chapter: Entity,
    pub(super) turn: Entity,
    pub(super) total: Entity,
    pub(super) forecast: Entity,
    pub(super) estimate: Entity,
    pub(super) until: Entity,
    pub(super) offer_count: Entity,
    pub(super) offers: [Entity; 3],
    pub(super) info_title: Entity,
    pub(super) info_desc: Entity,
    pub(super) info_rule: Entity,
    pub(super) preview: Entity,
    pub(super) actions: [Entity; 5],
    pub(super) goal: Entity,
    pub(super) goal_progress: Entity,
    pub(super) message: Entity,
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) modal_score: Entity,
    pub(super) modal_detail: Entity,
    pub(super) perk_buttons: [Entity; 3],
    pub(super) finish: Entity,
    pub(super) voyage_rows: [Entity; 4],
}

impl TideNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut TideModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<TideModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        Self::apply_changes(world, changes);
    }

    pub(super) fn dispatch(world: &mut World, command: TideCommand) {
        #[cfg(feature = "persistence")]
        if world
            .resource::<TidalReplayLog>()
            .is_none_or(TidalReplayLog::is_full)
        {
            return;
        }
        let changes = world
            .resource_mut::<TideModel>()
            .map(|model| model.apply_command(command))
            .unwrap_or(ChangeSet::NONE);
        #[cfg(feature = "persistence")]
        if changes.contains(ChangeSet::PERSISTENCE) {
            let recorded = world
                .resource_mut::<TidalReplayLog>()
                .is_some_and(|log| log.record_tide(command).is_ok());
            debug_assert!(recorded);
        }
        Self::apply_changes(world, changes);
    }

    pub(super) fn dispatch_pending(world: &mut World) {
        let Some((index, choice)) = world
            .resource::<TideModel>()
            .and_then(|model| model.pending().map(|index| (index, model.choice())))
        else {
            return;
        };
        Self::dispatch(world, TideCommand::Place { index, choice });
    }

    fn apply_changes(world: &mut World, changes: ChangeSet) {
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
        let Some(model) = world.resource::<TideModel>() else {
            return;
        };
        let forecast = model.forecast();
        let pending = model.pending();
        let selected = model.selected();
        let choice = model.choice();
        let modal = model.modal();
        let display_tile = selected
            .map(|index| model.tile(usize::from(index)))
            .unwrap_or_else(|| model.offer(usize::from(choice)));
        let preview =
            pending.and_then(|index| model.preview(usize::from(index), usize::from(choice)));
        let goal = model.goal();
        let texts = [
            (nodes.chapter, format!("第 {} / 4 岛", model.chapter() + 1)),
            (nodes.turn, format!("落子 {} / 24", model.turn())),
            (nodes.total, format!("累计 {}", model.cumulative_score())),
            (
                nodes.forecast,
                format!(
                    "{} · {}",
                    if forecast.tide == TideLevel::High {
                        "涨潮"
                    } else {
                        "退潮"
                    },
                    forecast.weather.name()
                ),
            ),
            (nodes.estimate, format!("预计 +{}", model.forecast_score())),
            (
                nodes.until,
                format!(
                    "再落 {} 块结算 · 进阶参考线 {}",
                    if model.settled() {
                        0
                    } else {
                        6 - model.turn() % 6
                    },
                    model.target()
                ),
            ),
            (nodes.offer_count, format!("换牌 {}", model.rerolls())),
            (
                nodes.info_title,
                format!(
                    "{} · {}",
                    if selected.is_some() {
                        "已落位"
                    } else {
                        "待落位"
                    },
                    display_tile.name()
                ),
            ),
            (nodes.info_desc, display_tile.description().into()),
            (nodes.info_rule, display_tile.rule().into()),
            (
                nodes.preview,
                preview.map_or_else(String::new, |(score, delta)| {
                    let terrain = model.terrain(usize::from(pending.expect("preview index")));
                    format!(
                        "{} · 本块 {} · 全岛净增 {:+}",
                        ["低地", "平地", "高地"][usize::from(terrain)],
                        score,
                        delta
                    )
                }),
            ),
            (nodes.goal, goal.name.into()),
            (
                nodes.goal_progress,
                format!("{} / {}", model.goal_progress(), goal.count),
            ),
            (nodes.message, message_text(model.message()).into()),
        ];
        let offers = [model.offer(0), model.offer(1), model.offer(2)];
        let modal_score = model.score();
        let harvests = (0..usize::from(model.harvest_len()))
            .map(|index| model.harvest(index))
            .fold(String::new(), |mut text, value| {
                if !text.is_empty() {
                    text.push_str(" / ");
                }
                use core::fmt::Write;
                let _ = write!(text, "{value}");
                text
            });
        let voyage_rows = core::array::from_fn::<_, 4, _>(|index| {
            if index < usize::from(model.chapter()) {
                let result = model.result(index);
                format!("0{}  {}  {}", index + 1, island_name(index), result.score)
            } else if index == usize::from(model.chapter()) {
                format!("0{}  {}  航行中", index + 1, island_name(index))
            } else {
                format!("0{}  {}  未抵达", index + 1, island_name(index))
            }
        });
        let is_result = modal == TideModal::Result;
        let is_voyage = modal == TideModal::Voyage;
        let final_island = model.chapter() == 3;
        let settled = model.settled();
        let complete = model.complete();
        let history = model.history_len();
        let _ = model;

        for (entity, content) in texts {
            set_text(world, entity, content);
        }
        for (index, entity) in nodes.offers.into_iter().enumerate() {
            set_text(world, entity, offers[index].name());
            set_button_state(
                world,
                entity,
                index == usize::from(choice),
                !settled && !complete,
            );
        }
        set_button_state(
            world,
            nodes.actions[0],
            false,
            !settled && !complete && rerolls(world) > 0,
        );
        set_button_state(world, nodes.actions[1], false, history > 0 && !complete);
        set_hidden(
            world,
            nodes.actions[0],
            pending.is_some() || settled || complete,
        );
        set_hidden(
            world,
            nodes.actions[1],
            pending.is_some() || settled || complete,
        );
        set_hidden(
            world,
            nodes.actions[2],
            pending.is_some() || settled || complete,
        );
        set_hidden(world, nodes.actions[3], pending.is_none());
        set_hidden(
            world,
            nodes.actions[4],
            pending.is_none() && !settled && !complete,
        );
        if pending.is_none() && (settled || complete) {
            set_text(
                world,
                nodes.actions[4],
                if complete {
                    "航行完成 · 查看总览"
                } else {
                    "本岛结算 · 选择新学说"
                },
            );
        } else {
            set_text(world, nodes.actions[4], "取消");
        }

        set_hidden(world, nodes.modal, modal == TideModal::None);
        set_text(
            world,
            nodes.modal_title,
            if is_voyage {
                "四岛航行图"
            } else if complete {
                "四岛远航 / 完成"
            } else {
                "本岛结算"
            },
        );
        set_text(
            world,
            nodes.modal_subtitle,
            if is_voyage {
                "96 次落子 / 16 次季节结算 / 3 次学说选择"
            } else {
                "评级不锁关。选择学说，继续前往下一座岛。"
            },
        );
        set_text(
            world,
            nodes.modal_score,
            if is_result {
                format!("{modal_score}")
            } else {
                String::new()
            },
        );
        set_text(
            world,
            nodes.modal_detail,
            if is_result {
                format!("四季 {harvests}")
            } else {
                String::new()
            },
        );
        set_hidden(world, nodes.modal_score, !is_result);
        set_hidden(world, nodes.modal_detail, !is_result);
        for (index, entity) in nodes.perk_buttons.into_iter().enumerate() {
            let visible = is_result && settled && !final_island && !complete;
            set_hidden(world, entity, !visible);
            if visible {
                let perk = perk_offer(world, index);
                set_text(
                    world,
                    entity,
                    format!("{}\n{}", perk.name(), perk.description()),
                );
            }
        }
        set_hidden(
            world,
            nodes.finish,
            !(is_result && final_island && settled && !complete),
        );
        for (index, entity) in nodes.voyage_rows.into_iter().enumerate() {
            set_hidden(world, entity, !is_voyage);
            if is_voyage {
                set_text(world, entity, voyage_rows[index].clone());
            }
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
            PAPER.into()
        } else if enabled {
            PANEL.into()
        } else {
            Color::rgb(29, 57, 63).into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            DARK.into()
        } else if enabled {
            TEXT.into()
        } else {
            MUTED.into()
        };
    }
    world.invalidate_visual(entity);
}

fn rerolls(world: &World) -> u8 {
    world.resource::<TideModel>().map_or(0, TideModel::rerolls)
}
pub(super) fn perk_offer(world: &World, index: usize) -> Perk {
    world
        .resource::<TideModel>()
        .expect("Tide model")
        .perk_offer(index)
}

fn message_text(message: TideMessage) -> &'static str {
    match message {
        TideMessage::Ready => "选一张地块，再点相邻海域。每 6 次落子结算一次。",
        TideMessage::Placed(_) => "地块已落位，候选与下一季预估已更新。",
        TideMessage::Harvest(_, _) => "季节结算完成；继续扩建群岛。",
        TideMessage::Rerolled => "换一手地块；不会消耗落子回合。",
        TideMessage::Undone => "已撤销；候选、积分与随机状态完整恢复。",
        TideMessage::Settled(true) => "委托达成，追加 45 分。",
        TideMessage::Settled(false) => "本岛结算；未完成委托不影响继续远航。",
        TideMessage::Complete => "四岛航行完成。",
    }
}

fn island_name(index: usize) -> &'static str {
    ["浅湾", "外海", "浮岬", "远境"][index]
}
