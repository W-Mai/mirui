use alloc::rc::Rc;
use core::cell::RefCell;

use crate::core::model::Model;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    BoardMotion, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel,
    ResultStars, start_result_stars,
};
use crate::gallery::play::fold::{FoldBoard, FoldModel};
use crate::prelude::*;

#[crate::component]
pub(super) struct FoldSurface {
    pub(super) game: model!(FoldModel),
    pub(super) expedition: model!(ExpeditionUiModel),
}

/// The map as a tap target: a tap rolls the hull toward it.
#[crate::component]
pub(super) struct MapTarget {
    pub(super) game: model!(FoldModel),
    pub(super) expedition: model!(ExpeditionUiModel),
}

/// What the map showed last frame: the director compares it with the
/// model, and the view rolls the hull over from the cells in `from`.
#[crate::component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FoldTrack {
    pub(super) board: FoldBoard,
    pub(super) from: ([u8; 2], u8),
}

impl Default for FoldTrack {
    fn default() -> Self {
        Self {
            board: FoldBoard::UNSEEN,
            from: ([0; 2], 1),
        }
    }
}

#[derive(Clone)]
pub(super) struct FoldHintService(Rc<RefCell<ExpeditionHintWorkspace>>);

impl FoldHintService {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(ExpeditionHintWorkspace::new())))
    }

    pub(super) fn request(&self, game: &<FoldModel as Model>::Handle) {
        game.request_hint(&mut self.0.borrow_mut());
    }
}

/// Starts the motion for whatever changed on the map since last frame: the
/// hull rolls over or bumps against a refused roll, a new level shows its
/// banner, a result pops its stars.
#[system]
pub(super) fn direct_fold(
    game: model!(FoldModel),
    expedition: model!(ExpeditionUiModel),
    maps: query!(Entity, FoldTrack),
    stars: query!(Entity, ResultStars),
    commands: commands!(),
) {
    let now = game.board();
    let mut earned = None;
    for (entity, track) in maps {
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
            from = (track.board.cells, track.board.len);
            BoardMotion::travel(&mut commands, entity);
        } else if cues.settle {
            from = (now.cells, now.len);
            BoardMotion::settle(&mut commands, entity);
        }
        if cues.bump {
            BoardMotion::bump(&mut commands, entity);
        }
        earned = earned.or(cues.stars);
        commands.insert(entity, FoldTrack { board: now, from });
    }
    if let Some(earned) = earned {
        for (entity, row) in stars {
            start_result_stars(&mut commands, entity, *row, earned);
        }
    }
}

/// Leaves a result: on to the next level, the next chapter's briefing, or
/// the log after the last one.
pub(super) fn next(
    game: &<FoldModel as Model>::Handle,
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
    game: &<FoldModel as Model>::Handle,
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
