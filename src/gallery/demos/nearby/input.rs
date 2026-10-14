use super::model::DisplayState;
use crate::prelude::*;
use crate::ui::widgets::FollowBounds;

#[event]
pub(super) fn place(
    target: entity!(),
    frame: Entity,
    state: model!(DisplayState),
    index: usize,
    frames: query!(mut FollowBounds),
) {
    if state.selected() == index
        && let Some(bounds) = frames.get(frame)
    {
        bounds.target = target;
    }
}

#[event]
pub(super) fn hover(
    event: event!(HoverChanged),
    target: entity!(),
    frame: Entity,
    state: model!(DisplayState),
    index: usize,
    frames: query!(mut FollowBounds),
) {
    if event.hovered {
        state.select(index);
        if let Some(bounds) = frames.get(frame) {
            bounds.target = target;
        }
    }
}
