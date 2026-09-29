use super::state::FoldHintService;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModelHandle};
use crate::gallery::play::fold::FoldModelHandle;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;

pub(super) fn move_model(game: &FoldModelHandle, direction: Direction4) {
    game.move_direction(direction);
}

pub(super) struct FoldKeyboardPlugin {
    game: FoldModelHandle,
    expedition: ExpeditionUiModelHandle,
    hints: FoldHintService,
}

impl FoldKeyboardPlugin {
    pub(super) fn new(
        game: FoldModelHandle,
        expedition: ExpeditionUiModelHandle,
        hints: FoldHintService,
    ) -> Self {
        Self {
            game,
            expedition,
            hints,
        }
    }
}

impl<B, F> Plugin<B, F> for FoldKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        if self.expedition.panel() != ExpeditionPanel::None {
            return false;
        }
        match ch {
            'w' | 'W' => move_model(&self.game, Direction4::Up),
            'd' | 'D' => move_model(&self.game, Direction4::Right),
            's' | 'S' => move_model(&self.game, Direction4::Down),
            'a' | 'A' => move_model(&self.game, Direction4::Left),
            'z' | 'Z' => {
                self.game.undo();
            }
            'h' | 'H' => self.hints.request(&self.game),
            _ => return false,
        }
        true
    }
}
