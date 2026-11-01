use super::data::Page;
use super::input::{SelectionFrame, SelectionTarget, contains};
use super::model::DisplayState;
use super::pixel::{ActorAnchor, ActorSprite, Stage};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::FollowBounds;
use crate::ui::{ComputedRect, Hidden, Parent};

#[component]
pub(super) struct ActorFrame;

#[system(order = Animation)]
pub(super) fn follow(
    state: model!(DisplayState),
    frames: query!(Entity, ActorFrame, optional Hidden, optional FollowBounds),
    anchors: query!(Entity, ActorAnchor, ComputedRect),
    targets: query!(Entity, SelectionTarget, ComputedRect),
    pages: query!(SelectionFrame),
    parents: query!(Parent),
    commands: commands!(),
) {
    let page = state.page_key();
    let session = matches!(page.page, Page::Host | Page::Joined | Page::Leaving);
    let root = pages
        .into_iter()
        .find_map(|page_frame| (page_frame.page == page).then_some(page_frame.body));
    let inside = |entity| {
        root.is_some_and(|root| contains(root, entity, |entity| parents.get(entity).map(|p| p.0)))
    };
    let anchor = anchors.into_iter().find_map(|(entity, anchor, _)| {
        let empty = if matches!(page.page, Page::Games | Page::Candidates) {
            state.presentation().entries_empty
        } else {
            state.presentation().rooms_empty
        };
        (inside(entity) && (!matches!(anchor.0, Stage::Search) || empty)).then_some(entity)
    });
    let target = anchor.or_else(|| {
        targets.into_iter().find_map(|(entity, row, _)| {
            (inside(entity) && row.index == state.selected()).then_some(entity)
        })
    });
    if state.actor_stage() != anchor.is_some() {
        state.set_actor_stage(anchor.is_some());
    }
    for (entity, _, hidden, bounds) in frames {
        if let Some(target) = target.filter(|_| !session && !state.launcher().active()) {
            if bounds.is_none_or(|old| old.target != target) {
                commands.insert(
                    entity,
                    FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
                );
            }
            if hidden.is_some() {
                commands.remove::<Hidden>(entity);
            }
        } else if hidden.is_none() {
            commands.insert(entity, Hidden);
        }
    }
}

#[compose]
pub(super) fn actor(state: model!(DisplayState)) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            ActorFrame,
            crate::ui::IgnoreHitTest,
        ] {
            View (
                position: Position::Absolute,
                left: ${
                    Dimension::Percent(
                        if state.actor_stage() {
                            state.actor_position() * 100
                        } else {
                            Fixed::from_int(86)
                        },
                    )
                },
                top: ${ Dimension::percent(if state.actor_stage() { 100 } else { 85 }) },
                width: 32,
                height: 48,
                transform: ${ Transform::translate(Fixed::from_int(-16), Fixed::from_int(-48)) }
            ) [
                ActorSprite {
                    state: state.clone(),
                },
            ]
        }
    }
}
