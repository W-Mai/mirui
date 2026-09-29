use super::state::TwinHintService;
use crate::core::model::Model;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModel};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;

pub(super) struct TwinKeyboardPlugin {
    game: <TwinModel as Model>::Handle,
    expedition: <ExpeditionUiModel as Model>::Handle,
    hints: TwinHintService,
}

impl TwinKeyboardPlugin {
    pub(super) fn new(
        game: <TwinModel as Model>::Handle,
        expedition: <ExpeditionUiModel as Model>::Handle,
        hints: TwinHintService,
    ) -> Self {
        Self {
            game,
            expedition,
            hints,
        }
    }
}

impl<B, F> Plugin<B, F> for TwinKeyboardPlugin
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
            'w' | 'W' => {
                self.game.move_direction(Direction4::Up);
            }
            'd' | 'D' => {
                self.game.move_direction(Direction4::Right);
            }
            's' | 'S' => {
                self.game.move_direction(Direction4::Down);
            }
            'a' | 'A' => {
                self.game.move_direction(Direction4::Left);
            }
            'z' | 'Z' => {
                self.game.undo();
            }
            'h' | 'H' => self.hints.request(&self.game),
            _ => return false,
        }
        true
    }
}
