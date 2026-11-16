use alloc::rc::Rc;
use core::cell::RefCell;

use crate::core::model::Model;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    BoardMotion, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel,
    ResultStars, start_result_stars,
};
use crate::gallery::play::twin::{TwinBoard, TwinModel};
use crate::prelude::*;

#[crate::component]
pub(super) struct TwinSurface {
    pub(super) game: model!(TwinModel),
    pub(super) expedition: model!(ExpeditionUiModel),
}

/// One station's board as a tap target: a tap moves its beacon one cell
/// toward the tap.
#[crate::component]
pub(super) struct StationTarget {
    pub(super) game: model!(TwinModel),
    pub(super) expedition: model!(ExpeditionUiModel),
    pub(super) station: u8,
}

/// What the boards showed last frame: the director compares it with the
/// model, and the view draws the beacons travelling from `from`.
#[crate::component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TwinTrack {
    pub(super) board: TwinBoard,
    pub(super) from: [u8; 2],
}

impl Default for TwinTrack {
    fn default() -> Self {
        Self {
            board: TwinBoard::UNSEEN,
            from: [0; 2],
        }
    }
}

#[derive(Clone)]
pub(super) struct TwinHintService(Rc<RefCell<ExpeditionHintWorkspace>>);

impl TwinHintService {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(ExpeditionHintWorkspace::new())))
    }

    pub(super) fn request(&self, game: &<TwinModel as Model>::Handle) {
        game.request_hint(&mut self.0.borrow_mut());
    }
}

/// Starts the motion for whatever changed on the boards since last frame:
/// the beacons travel a cell or bump against a refused move, a new level
/// shows its banner, a result pops its stars.
#[system]
pub(super) fn direct_twin(
    game: model!(TwinModel),
    expedition: model!(ExpeditionUiModel),
    boards: query!(Entity, TwinTrack),
    stars: query!(Entity, ResultStars),
    commands: commands!(),
) {
    let now = game.board();
    let mut earned = None;
    for (entity, track) in boards {
        if track.board == now {
            continue;
        }
        let cues = track.board.shot.cues(now.shot);
        let mut from = track.from;
        if cues.new_level && expedition.panel() == ExpeditionPanel::None {
            expedition.show_banner();
            BoardMotion::banner(&mut commands, entity);
        }
        if cues.travel || cues.bump {
            expedition.hide_banner();
        }
        if cues.travel {
            from = track.board.cells;
            BoardMotion::travel(&mut commands, entity);
        } else if cues.settle {
            from = now.cells;
            BoardMotion::settle(&mut commands, entity);
        }
        if cues.bump {
            BoardMotion::bump(&mut commands, entity);
        }
        earned = earned.or(cues.stars);
        commands.insert(entity, TwinTrack { board: now, from });
    }
    if let Some(earned) = earned {
        for (entity, row) in stars {
            start_result_stars(&mut commands, entity, *row, earned);
        }
    }
}

/// Leaves a result: on to the next station, the next chapter's briefing,
/// or the log after the last one.
pub(super) fn next(
    game: &<TwinModel as Model>::Handle,
    expedition: &<ExpeditionUiModel as Model>::Handle,
) {
    let level = game.level_index();
    match game.modal() {
        ExpeditionModal::Final => {
            expedition.open_summary();
        }
        ExpeditionModal::Result => {
            game.continue_campaign();
            if level % 6 == 5 {
                expedition.open_briefing();
            }
        }
        ExpeditionModal::None => {}
    }
}

pub(super) fn select_map_level(
    game: &<TwinModel as Model>::Handle,
    expedition: &<ExpeditionUiModel as Model>::Handle,
    slot: u8,
) {
    let level = expedition.chapter() * 6 + slot;
    if level > game.unlocked() {
        return;
    }
    let briefing = level > 0 && level % 6 == 0 && !game.progress().completed(level);
    // Picking the level already in play just goes back to it.
    if game.select_level(level) == ChangeSet::NONE && level != game.level_index() {
        return;
    }
    if briefing {
        expedition.open_briefing();
    } else {
        expedition.close();
    }
}
