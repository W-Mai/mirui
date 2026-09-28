use super::state::FoldNodes;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiState};
use crate::gallery::play::fold::FoldModel;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;

pub(super) fn move_model(world: &mut World, direction: Direction4) {
    FoldNodes::update(world, |model| model.move_direction(direction));
}

pub(super) struct FoldKeyboardPlugin;

impl<B, F> Plugin<B, F> for FoldKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        if world
            .resource::<ExpeditionUiState>()
            .is_some_and(|state| state.panel() != ExpeditionPanel::None)
        {
            return false;
        }
        match ch {
            'w' | 'W' => move_model(world, Direction4::Up),
            'd' | 'D' => move_model(world, Direction4::Right),
            's' | 'S' => move_model(world, Direction4::Down),
            'a' | 'A' => move_model(world, Direction4::Left),
            'z' | 'Z' => FoldNodes::update(world, FoldModel::undo),
            'h' | 'H' => FoldNodes::hint(world),
            _ => return false,
        }
        true
    }
}
