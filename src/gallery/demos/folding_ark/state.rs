use alloc::rc::Rc;
use core::cell::RefCell;

use crate::core::model::Model;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    ExpeditionHintWorkspace, ExpeditionModal, ExpeditionUiModel,
};
use crate::gallery::play::fold::FoldModel;

#[crate::component(bind(game, expedition))]
pub(super) struct FoldSurface {
    pub(super) game: FoldModel,
    pub(super) expedition: ExpeditionUiModel,
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
        ExpeditionModal::None => {
            let next = if level < game.unlocked() {
                level + 1
            } else {
                0
            };
            game.select_level(next);
        }
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
    if game.select_level(level) == ChangeSet::NONE {
        return;
    }
    if briefing {
        expedition.open_briefing();
    } else {
        expedition.close();
    }
}
