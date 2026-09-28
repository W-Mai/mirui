use alloc::{format, string::String};

use super::style::{HEADER, MUTED, PANEL, TEXT};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::echo::{EchoCommand, EchoMessage, EchoModal, EchoModel};
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::EchoReplayLog;
use crate::prelude::{Entity, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct EchoSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct EchoModalSurface;

#[derive(Clone, Copy)]
pub(super) struct EchoNodes {
    pub(super) surface: Entity,
    pub(super) room: Entity,
    pub(super) memory: Entity,
    pub(super) ghosts: Entity,
    pub(super) tick: Entity,
    pub(super) run: Entity,
    pub(super) message: Entity,
    pub(super) actions: [Entity; 4],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) modal_primary: Entity,
    pub(super) modal_secondary: Entity,
    pub(super) tape_rows: [Entity; 3],
    pub(super) result_stats: Entity,
}

impl EchoNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut EchoModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<EchoModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        Self::apply_changes(world, changes);
    }

    pub(super) fn dispatch(world: &mut World, command: EchoCommand) {
        #[cfg(feature = "persistence")]
        if world
            .resource::<EchoReplayLog>()
            .is_none_or(EchoReplayLog::is_full)
        {
            return;
        }
        let changes = world
            .resource_mut::<EchoModel>()
            .map(|model| model.apply_command(command))
            .unwrap_or(ChangeSet::NONE);
        #[cfg(feature = "persistence")]
        if changes.contains(ChangeSet::PERSISTENCE) {
            let recorded = world
                .resource_mut::<EchoReplayLog>()
                .is_some_and(|log| log.record_echo(command).is_ok());
            debug_assert!(recorded);
        }
        Self::apply_changes(world, changes);
    }

    fn apply_changes(world: &mut World, changes: ChangeSet) {
        if changes.contains(ChangeSet::VISUAL)
            && let Some(nodes) = world.resource::<Self>().copied()
        {
            world.invalidate_visual(nodes.surface);
            world.invalidate_visual(nodes.modal);
        }
        if changes.contains(ChangeSet::MODEL) {
            Self::sync(world);
        }
    }

    pub(super) fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<EchoModel>() else {
            return;
        };
        let modal = model.modal();
        let complete = model.complete();
        let won = model.won();
        let ghost_count = model.ghost_count();
        let route_len = model.route_len();
        let history_len = model.history_len();
        let peek = model.peek_ghost();
        let result_len = model.result_len();
        let result_stats = if complete {
            format!(
                "TOTAL {} STEPS · {} REWINDS",
                model.total_steps(),
                model.total_loops()
            )
        } else {
            format!("ROOM {} STEPS · {} REWINDS", model.steps(), model.loops())
        };
        let texts = [
            (nodes.room, format!("ARCHIVE {:02} / 12", model.level() + 1)),
            (
                nodes.memory,
                format!(
                    "MEMORY {}/{}",
                    model.collected_count(),
                    model.room().gem_count
                ),
            ),
            (nodes.ghosts, format!("ECHOES {ghost_count}/3")),
            (nodes.tick, format!("{:02}", model.tick())),
            (
                nodes.run,
                format!("{} STEPS / {} ECHOES", model.steps(), model.loops()),
            ),
            (nodes.message, message_text(model.message())),
            (nodes.result_stats, result_stats),
        ];
        let _ = model;
        for (entity, content) in texts {
            set_text(world, entity, content);
        }
        set_button_state(
            world,
            nodes.actions[0],
            route_len > 0 && ghost_count < 3 && !won,
        );
        set_button_state(world, nodes.actions[1], history_len > 0 && !complete);
        set_button_state(world, nodes.actions[2], route_len > 0 && !won);
        set_button_state(world, nodes.actions[3], true);
        set_hidden(world, nodes.modal, modal == EchoModal::None);
        let tapes = modal == EchoModal::Tapes;
        let result = modal == EchoModal::Result;
        set_text(
            world,
            nodes.modal_title,
            if tapes {
                "ECHO TAPES"
            } else if complete {
                "TWELVE ROOMS RESTORED"
            } else {
                "ARCHIVE RESTORED"
            },
        );
        set_text(
            world,
            nodes.modal_subtitle,
            if tapes {
                "Inspect each route. An echo holds its final cell."
            } else if complete {
                "All twelve rooms are restored."
            } else {
                "Past and present completed the route together."
            },
        );
        for (index, row) in nodes.tape_rows.into_iter().enumerate() {
            set_hidden(world, row, !tapes || index >= usize::from(ghost_count));
            set_text(
                world,
                row,
                format!(
                    "ECHO {} · {:02} BEATS{}",
                    index + 1,
                    ghost_route_len(world, index),
                    if peek == Some(index as u8) {
                        " · SOLO"
                    } else {
                        ""
                    }
                ),
            );
            set_button_state(world, row, true);
        }
        set_hidden(world, nodes.result_stats, !result);
        set_hidden(world, nodes.modal_primary, !result);
        set_hidden(world, nodes.modal_secondary, !tapes);
        set_text(
            world,
            nodes.modal_primary,
            if complete {
                "ARCHIVE COMPLETE"
            } else if result_len == 11 {
                "COMPLETE ARCHIVE"
            } else {
                "NEXT ROOM"
            },
        );
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<String>) {
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

fn set_button_state(world: &mut World, entity: Entity, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if enabled { PANEL } else { HEADER }.into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if enabled { TEXT } else { MUTED }.into();
    }
    world.invalidate_visual(entity);
}

fn ghost_route_len(world: &World, ghost: usize) -> u8 {
    world
        .resource::<EchoModel>()
        .map_or(0, |model| model.ghost_route_len(ghost))
}

fn message_text(message: EchoMessage) -> String {
    match message {
        EchoMessage::Ready => "Reach a plate, rewind, and let the past replay the route.".into(),
        EchoMessage::Plate(index) => format!(
            "PLATE {} HELD · RECORD THIS ROUTE",
            char::from(b'A' + index)
        ),
        EchoMessage::Fragment => "MEMORY FRAGMENT STORED ACROSS REWINDS".into(),
        EchoMessage::Missing(count) => format!("EXIT NEEDS {count} MORE MEMORY FRAGMENTS"),
        EchoMessage::BeatLimit => "48 BEATS USED · RECORD OR RESTART THIS ROUTE".into(),
        EchoMessage::Recorded => "ECHO RECORDED · IT WILL HOLD THE FINAL CELL".into(),
        EchoMessage::Restarted => "ROUTE RESTARTED · ECHOES AND MEMORY REMAIN".into(),
        EchoMessage::Cleared => "ROOM RESET · SEED AND GEOMETRY PRESERVED".into(),
        EchoMessage::Undone => "UNDO RESTORED TIME, MEMORY, AND ECHOES".into(),
        EchoMessage::Won => "ARCHIVE RESTORED · CONTINUE TO THE NEXT ROOM".into(),
        EchoMessage::Complete => "ALL TWELVE ROOMS RESTORED".into(),
    }
}
