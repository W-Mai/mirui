use crate::core::model::Model;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    BoardMotion, BoardShot, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, ResultStars,
    start_result_stars,
};
use crate::gallery::play::picture::PictureModel;
use crate::prelude::*;

#[crate::component]
pub(super) struct PictureSurface {
    pub(super) model: model!(PictureModel),
    pub(super) expedition: model!(ExpeditionUiModel),
}

/// What the board showed last frame, for the director.
#[crate::component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PictureTrack(pub(super) BoardShot);

impl Default for PictureTrack {
    fn default() -> Self {
        Self(BoardShot::UNSEEN)
    }
}

/// A new picture shows its banner until the first stroke; a restored one
/// pops its stars.
#[system]
pub(super) fn direct_picture(
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
    boards: query!(Entity, PictureTrack),
    stars: query!(Entity, ResultStars),
    commands: commands!(),
) {
    let now = model.board();
    let mut earned = None;
    for (entity, track) in boards {
        if track.0 == now {
            continue;
        }
        let cues = track.0.cues(now);
        if cues.new_level && expedition.panel() == ExpeditionPanel::None {
            expedition.show_banner();
            BoardMotion::banner(&mut commands, entity);
        } else if !cues.new_level && track.0.steps != now.steps {
            expedition.hide_banner();
        }
        earned = earned.or(cues.stars);
        commands.insert(entity, PictureTrack(now));
    }
    if let Some(earned) = earned {
        for (entity, row) in stars {
            start_result_stars(&mut commands, entity, *row, earned);
        }
    }
}

/// Leaves a result: on to the next picture, the next chapter's briefing,
/// or the log after the last one.
pub(super) fn next(
    model: &<PictureModel as Model>::Handle,
    expedition: &<ExpeditionUiModel as Model>::Handle,
) {
    let level = model.level_index();
    match model.modal() {
        ExpeditionModal::Final => {
            expedition.open_summary();
        }
        ExpeditionModal::Result => {
            model.continue_campaign();
            if level % 6 == 5 {
                expedition.open_briefing();
            }
        }
        ExpeditionModal::None => {}
    }
}

pub(super) fn select_map_level(
    model: &<PictureModel as Model>::Handle,
    expedition: &<ExpeditionUiModel as Model>::Handle,
    slot: u8,
) {
    let level = expedition.chapter() * 6 + slot;
    if level > model.unlocked() {
        return;
    }
    let briefing = level > 0 && level % 6 == 0 && !model.progress().completed(level);
    // Picking the picture already open just goes back to it.
    if model.select_level(level) == ChangeSet::NONE && level != model.level_index() {
        return;
    }
    if briefing {
        expedition.open_briefing();
    } else {
        expedition.close();
    }
}
