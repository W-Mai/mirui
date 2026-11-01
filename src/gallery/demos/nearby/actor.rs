use super::data::Page;
use super::input::{SelectionFrame, SelectionTarget, contains};
use super::model::DisplayState;
use super::pixel::{ActorAnchor, ActorSprite, Stage};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::FollowBounds;
use crate::ui::{Hidden, Parent};

#[component]
pub(super) struct ActorFrame {
    pub launcher: bool,
    pub state: model!(DisplayState),
}

fn root<'a, 'b>(
    state: &super::model::DisplayStateHandle,
    pages: impl IntoIterator<Item = &'a SelectionFrame>,
    libraries: impl IntoIterator<Item = &'b super::library::LibraryFocus>,
) -> Option<Entity> {
    let library = state.launcher();
    if library.active() {
        libraries
            .into_iter()
            .find_map(|frame| (frame.screen == library.screen()).then_some(frame.body))
    } else {
        pages.into_iter().find_map(|frame| {
            (frame.page == state.page_key() && frame.room == state.return_to_launcher())
                .then_some(frame.body)
        })
    }
}

fn resolve<'a, 'b, 'c, 'd>(
    state: &super::model::DisplayStateHandle,
    root: Option<Entity>,
    anchors: impl IntoIterator<Item = (Entity, &'a ActorAnchor)>,
    targets: impl IntoIterator<Item = (Entity, &'b SelectionTarget)>,
    library_targets: impl IntoIterator<Item = (Entity, &'c super::library::LibraryTarget)>,
    action_targets: impl IntoIterator<Item = (Entity, &'d super::actions::ActionAnchor)>,
    parent: impl Fn(Entity) -> Option<Entity>,
) -> Option<(Entity, bool)> {
    let page = state.page_key();
    let library = state.launcher();
    if !library.active() && matches!(page.page, Page::Host | Page::Joined | Page::Leaving) {
        return None;
    }
    let inside = |entity| root.is_some_and(|root| contains(root, entity, &parent));
    let anchor = anchors.into_iter().find_map(|(entity, anchor)| {
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
                .find_map(|(entity, row)| (row.0 == library.menu_selection()).then_some(entity))
        } else {
            anchor.or_else(|| {
                if library.screen() == super::launcher::Screen::Search && library.search_empty() {
                    None
                } else {
                    library_targets.into_iter().find_map(|(entity, row)| {
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
            targets.into_iter().find_map(|(entity, row)| {
                (inside(entity) && row.active(state) && row.index == state.selected())
                    .then_some(entity)
            })
        })
    };
    target.map(|target| (target, anchor.is_some()))
}

#[event]
fn initialize(
    node: entity!(),
    frames: query!(ActorFrame),
    anchors: query!(Entity, ActorAnchor),
    targets: query!(Entity, SelectionTarget),
    pages: query!(SelectionFrame),
    libraries: query!(super::library::LibraryFocus),
    library_targets: query!(Entity, super::library::LibraryTarget),
    action_targets: query!(Entity, super::actions::ActionAnchor),
    parents: query!(Parent, optional super::input::PaneScope),
    commands: commands!(),
) {
    let Some(frame) = frames.get(node) else {
        return;
    };
    let state = &frame.state;
    if frame.launcher != (state.launcher().active() || state.return_to_launcher()) {
        commands.insert(node, Hidden);
        return;
    }
    let target = resolve(
        state,
        root(state, pages, libraries),
        anchors,
        targets,
        library_targets,
        action_targets,
        |e| {
            parents.get(e).and_then(|(p, scope)| {
                scope
                    .is_none_or(|s| s.0 == state.focus_page())
                    .then_some(p.0)
            })
        },
    );
    if let Some((target, stage)) = target {
        state.set_actor_stage(stage);
        commands.remove::<Hidden>(node);
        commands.insert(
            node,
            FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
        );
    } else {
        commands.insert(node, Hidden);
    }
}

#[view(component = ActorFrame, attach = initialize::callback())]
pub(super) fn actor_frame() {}

#[system(order = Animation)]
pub(super) fn follow(
    state: model!(DisplayState),
    frames: query!(Entity, ActorFrame, optional Hidden, optional FollowBounds),
    anchors: query!(Entity, ActorAnchor),
    targets: query!(Entity, SelectionTarget),
    pages: query!(SelectionFrame),
    libraries: query!(super::library::LibraryFocus),
    library_targets: query!(Entity, super::library::LibraryTarget),
    action_targets: query!(Entity, super::actions::ActionAnchor),
    parents: query!(Parent, optional super::input::PaneScope),
    commands: commands!(),
) {
    let target = resolve(
        state,
        root(state, pages, libraries),
        anchors,
        targets,
        library_targets,
        action_targets,
        |e| {
            parents.get(e).and_then(|(p, scope)| {
                scope
                    .is_none_or(|s| s.0 == state.focus_page())
                    .then_some(p.0)
            })
        },
    );
    let stage = target.is_some_and(|(_, stage)| stage);
    if state.actor_stage() != stage {
        state.set_actor_stage(stage);
    }
    for (entity, marker, hidden, bounds) in frames {
        let active = marker.launcher == (state.launcher().active() || state.return_to_launcher());
        if let Some((target, _)) = target.filter(|_| active) {
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
            ActorFrame {
                launcher,
                state: state.clone(),
            },
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
