use super::data::Page;
use super::input::{SelectionFrame, SelectionTarget, contains};
use super::model::DisplayState;
use super::pixel::{ActorAnchor, ActorSprite, Stage};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::FollowBounds;
use crate::ui::{ComputedRect, Hidden, Parent};

#[component]
pub(super) struct ActorFrame {
    pub launcher: bool,
}

#[system(order = Animation)]
pub(super) fn follow(
    state: model!(DisplayState),
    frames: query!(Entity, ActorFrame, optional Hidden, optional FollowBounds),
    anchors: query!(Entity, ActorAnchor, ComputedRect),
    targets: query!(Entity, SelectionTarget, ComputedRect),
    pages: query!(SelectionFrame),
    library_frames: query!(super::library::LibraryFocus),
    library_targets: query!(Entity, super::library::LibraryTarget, ComputedRect),
    action_targets: query!(Entity, super::actions::ActionAnchor, ComputedRect),
    parents: query!(Parent, optional super::input::PaneScope),
    commands: commands!(),
) {
    let page = state.page_key();
    let library = state.launcher();
    let session =
        !library.active() && matches!(page.page, Page::Host | Page::Joined | Page::Leaving);
    let root = if library.active() {
        library_frames
            .into_iter()
            .find_map(|frame| (frame.screen == library.screen()).then_some(frame.body))
    } else {
        pages.into_iter().find_map(|page_frame| {
            (page_frame.page == page && page_frame.room == state.return_to_launcher())
                .then_some(page_frame.body)
        })
    };
    let inside = |entity| {
        root.is_some_and(|root| {
            contains(root, entity, |entity| {
                parents.get(entity).and_then(|(parent, pane)| {
                    pane.is_none_or(|pane| pane.0 == state.focus_page())
                        .then_some(parent.0)
                })
            })
        })
    };
    let anchor = anchors.into_iter().find_map(|(entity, anchor, _)| {
        let empty = if library.active() {
            !library.keyboard() && library.search_empty()
        } else if matches!(page.page, Page::Games | Page::Candidates) {
            state.presentation().entries_empty
        } else {
            state.presentation().rooms_empty
        };
        (inside(entity) && (!matches!(anchor.0, Stage::Search) || empty)).then_some(entity)
    });
    let target = if library.active() {
        if library.menu() {
            action_targets
                .into_iter()
                .find_map(|(entity, row, _)| (row.0 == library.menu_selection()).then_some(entity))
        } else {
            anchor.or_else(|| {
                if library.screen() == super::launcher::Screen::Search && library.search_empty() {
                    None
                } else {
                    library_targets.into_iter().find_map(|(entity, row, _)| {
                        (row.screen == library.screen()
                            && row.key == library.selected()
                            && inside(entity))
                        .then_some(entity)
                    })
                }
            })
        }
    } else {
        anchor.or_else(|| {
            targets.into_iter().find_map(|(entity, row, _)| {
                (inside(entity) && row.active(state) && row.index == state.selected())
                    .then_some(entity)
            })
        })
    };
    let stage = anchor.is_some();
    if state.actor_stage() != stage {
        state.set_actor_stage(stage);
    }
    for (entity, marker, hidden, bounds) in frames {
        let active = marker.launcher == (library.active() || state.return_to_launcher());
        if let Some(target) = target.filter(|_| !session && active) {
            if bounds.is_none_or(|old| old.target != target) {
                commands.insert(
                    entity,
                    FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
                );
            }
            if hidden.is_some() {
                commands.remove::<Hidden>(entity);
            }
        } else {
            if hidden.is_none() {
                commands.insert(entity, Hidden);
            }
            if bounds.is_some() {
                commands.remove::<FollowBounds>(entity);
            }
        }
    }
}

#[compose]
pub(super) fn actor(state: model!(DisplayState), launcher: bool) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            ActorFrame { launcher },
            crate::ui::IgnoreHitTest,
        ] {
            View (
                position: Position::Absolute,
                left: ${
                    Dimension::Percent(
                        if state.launcher().active()
                            && (state.launcher().screen() == super::launcher::Screen::Favorites
                                || state.launcher().keyboard()) && !state.launcher().menu()
                        {
                            Fixed::from_int(100)
                        } else if state.actor_stage() {
                            if state.launcher().active() {
                                Fixed::from_int(50)
                            } else {
                                state.actor_position() * 100
                            }
                        } else {
                            Fixed::from_int(86)
                        },
                    )
                },
                top: ${
                    Dimension::percent(
                        if state.actor_stage()
                            || (state.launcher().active()
                                && (state.launcher().screen() == super::launcher::Screen::Favorites
                                    || state.launcher().keyboard()) && !state.launcher().menu())
                        {
                            100
                        } else if state.launcher().active() && state.launcher().menu() {
                            50
                        } else {
                            85
                        },
                    )
                },
                width: 32,
                height: 48,
                transform: ${
                    let compact = state.launcher().active()
                        && (state.launcher().menu()
                            || state.launcher().screen() == super::launcher::Screen::Favorites
                            || state.launcher().keyboard());
                    Transform::translate(
                            Fixed::from_int(if compact && !state.launcher().menu() { -24 } else { -16 }),
                            Fixed::from_int(if compact { -36 } else { -48 }),
                        )
                        .compose(
                            &Transform::scale(
                                if compact { Fixed::HALF } else { Fixed::ONE },
                                if compact { Fixed::HALF } else { Fixed::ONE },
                            ),
                        )
                }
            ) [
                ActorSprite {
                    state: state.clone(),
                    actions: state.launcher(),
                },
            ]
        }
    }
}
