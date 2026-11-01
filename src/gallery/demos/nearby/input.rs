use super::model::{DisplayState, PageKey};
use crate::prelude::*;
use crate::ui::Parent;
use crate::ui::widgets::FollowBounds;

#[component]
pub(super) struct SelectionTarget {
    pub index: usize,
    pub scope: Option<super::data::Page>,
}

impl SelectionTarget {
    pub(super) fn active(&self, state: &super::model::DisplayStateHandle) -> bool {
        self.scope.is_none_or(|scope| {
            scope == state.focus_page()
                || scope == super::data::Page::Rooms
                    && state.focus_page() == super::data::Page::Lobby
        })
    }
}

#[component]
pub(super) struct PaneScope(pub super::data::Page);

#[component]
pub(super) struct SelectionFrame {
    pub body: Entity,
    pub page: PageKey,
    pub room: bool,
    pub state: model!(DisplayState),
}

#[event]
fn initialize_frame(
    node: entity!(),
    frames: query!(SelectionFrame),
    targets: query!(Entity, SelectionTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(frame) = frames.get(node) else {
        return;
    };
    if frame.state.launcher().active()
        || frame.page != frame.state.page_key()
        || frame.room != frame.state.return_to_launcher()
    {
        return;
    }
    if let Some(target) = targets.into_iter().find_map(|(entity, target)| {
        (target.active(&frame.state)
            && target.index == frame.state.selected()
            && contains(frame.body, entity, |e| parents.get(e).map(|p| p.0)))
        .then_some(entity)
    }) {
        commands.insert(
            node,
            FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
        );
    }
}

#[view(component = SelectionFrame, attach = initialize_frame::callback())]
pub(super) fn selection_frame() {}

#[system(order = Animation)]
pub(super) fn follow(
    state: model!(DisplayState),
    frames: query!(Entity, SelectionFrame, optional crate::ui::Hidden, optional FollowBounds),
    targets: query!(Entity, SelectionTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let mut active = None;
    let selectable = match state.presentation().page {
        super::data::Page::Rooms | super::data::Page::Lobby => state.rooms_visible(),
        super::data::Page::Games | super::data::Page::Candidates => {
            !state.presentation().entries_empty
        }
        super::data::Page::Host | super::data::Page::Joined => state.confirm_visible(),
        _ => true,
    };
    for (entity, frame, hidden, bounds) in frames {
        if selectable
            && !state.launcher().active()
            && frame.page == state.page_key()
            && frame.room == state.return_to_launcher()
        {
            active = Some((
                entity,
                frame.body,
                hidden.is_some(),
                bounds.map(|b| b.target),
            ));
        } else {
            if hidden.is_none() {
                commands.insert(entity, crate::ui::Hidden);
            }
            if bounds.is_some() {
                commands.remove::<FollowBounds>(entity);
            }
        }
    }
    let Some((frame, body, hidden, previous)) = active else {
        return;
    };
    let target = targets.into_iter().find_map(|(entity, marker)| {
        (marker.active(state)
            && marker.index == state.selected()
            && contains(body, entity, |e| parents.get(e).map(|p| p.0)))
        .then_some(entity)
    });
    if let Some(target) = target {
        if previous != Some(target) {
            commands.insert(
                frame,
                FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
            );
        }
        if hidden {
            commands.remove::<crate::ui::Hidden>(frame);
        }
    } else {
        if !hidden {
            commands.insert(frame, crate::ui::Hidden);
        }
        if previous.is_some() {
            commands.remove::<FollowBounds>(frame);
        }
    }
}

pub(super) fn contains(
    root: Entity,
    mut node: Entity,
    parent: impl Fn(Entity) -> Option<Entity>,
) -> bool {
    loop {
        if node == root {
            return true;
        }
        let Some(next) = parent(node) else {
            return false;
        };
        node = next;
    }
}

#[event]
pub(super) fn hover(
    event: event!(HoverChanged),
    target: entity!(),
    state: model!(DisplayState),
    index: usize,
    targets: query!(SelectionTarget),
) {
    if event.hovered {
        if targets
            .get(target)
            .is_none_or(|target| !target.active(state))
        {
            return;
        }
        state.select(index);
    }
}

#[event]
pub(super) fn keyboard(event: event!(Key), state: model!(DisplayState)) -> bool {
    use crate::input::event::input::{
        KEY_A, KEY_ESCAPE, KEY_LEFT, KEY_RETURN, KEY_RIGHT, KEY_X, PhysicalKey,
    };
    if event
        .modifiers
        .is_some_and(|m| m.meta || m.control || m.alt)
    {
        if state.launcher().holding() {
            state.launcher().cancel_hold();
        }
        if !event.pressed && event.physical == Some(PhysicalKey::KeyX) {
            state.launcher().release_hold();
        }
        return false;
    }
    let library = state.launcher();
    let code = match event.physical {
        Some(PhysicalKey::KeyA) => KEY_A,
        Some(PhysicalKey::KeyB) => 0x62,
        Some(PhysicalKey::KeyX) => KEY_X,
        Some(PhysicalKey::KeyY) => 0x79,
        _ => event.code,
    };
    if code == KEY_X && (library.active() || !event.pressed) {
        if event.pressed && event.repeat != Some(true) {
            super::launcher_view::begin_play(state);
        } else if !event.pressed {
            super::launcher_view::finish_play(state);
        }
        return true;
    }
    if !event.pressed {
        return false;
    }
    if event.physical == Some(PhysicalKey::F2) {
        state.toggle_controls();
        return true;
    }
    if code == 0x79 {
        if library.active() {
            if library.menu() {
                return true;
            }
            match library.screen() {
                super::launcher::Screen::Favorites => library.sort_favorites(),
                super::launcher::Screen::Platforms => library.open(super::launcher::Screen::Search),
                super::launcher::Screen::Search if library.keyboard() => library.type_key("退格"),
                super::launcher::Screen::Search => {
                    let query = library.query().to_lowercase();
                    if !query.trim().is_empty()
                        && library.games().iter().any(|game| {
                            game.key() as usize == library.selected() && game.matches(&query)
                        })
                    {
                        library.toggle_selected_favorite();
                    } else {
                        library.edit_query();
                    }
                }
                super::launcher::Screen::Games => library.toggle_selected_favorite(),
            }
        } else if state.can_switch_theme() {
            library.adjust(super::launcher::MenuAction::CycleTheme);
        }
        return true;
    }
    if library.active() {
        if event.physical == Some(PhysicalKey::Tab) {
            library.open(if library.screen() == super::launcher::Screen::Favorites {
                super::launcher::Screen::Platforms
            } else {
                super::launcher::Screen::Favorites
            });
            return true;
        }
        if event.physical == Some(PhysicalKey::F1) {
            if library.screen() == super::launcher::Screen::Search {
                if library.keyboard() {
                    library.type_key("结果");
                } else {
                    library.edit_query();
                }
            } else {
                library.show_overlay(super::launcher::Overlay::System);
            }
            return true;
        }
        if event.physical == Some(PhysicalKey::Space) {
            state.open_network();
            return true;
        }
        let direction = match event.physical {
            Some(PhysicalKey::ArrowLeft | PhysicalKey::ArrowUp) => Some(false),
            Some(PhysicalKey::ArrowRight | PhysicalKey::ArrowDown) => Some(true),
            _ => None,
        };
        if let Some(forward) = direction {
            if library.menu() {
                let delta = match event.physical {
                    Some(PhysicalKey::ArrowUp) => -2,
                    Some(PhysicalKey::ArrowDown) => 2,
                    Some(PhysicalKey::ArrowLeft) => -1,
                    _ => 1,
                };
                library.select_action((library.menu_selection() as i32 + delta).max(0) as usize);
                return true;
            }
            if library.keyboard() {
                let delta = match event.physical {
                    Some(PhysicalKey::ArrowUp) => -10,
                    Some(PhysicalKey::ArrowDown) => 10,
                    Some(PhysicalKey::ArrowLeft) => -1,
                    _ => 1,
                };
                library.select(
                    100 + library
                        .selected()
                        .saturating_sub(100)
                        .saturating_add_signed(delta)
                        .min(39),
                );
            } else {
                let vertical = matches!(
                    event.physical,
                    Some(PhysicalKey::ArrowUp | PhysicalKey::ArrowDown)
                );
                let step = if vertical && library.screen() == super::launcher::Screen::Favorites {
                    library.shelf().plan().grid.columns
                } else {
                    1
                } as isize;
                super::launcher_view::move_selection(state, if forward { step } else { -step });
            }
            return true;
        }
        match code {
            KEY_ESCAPE | 0x62 => library.back(),
            KEY_A | KEY_RETURN => super::launcher_view::confirm(state),
            _ => return false,
        }
        return true;
    }
    if event.physical == Some(PhysicalKey::KeyR) && state.focus_page() == super::data::Page::Lobby {
        state.expand_discovery(true);
        return true;
    }
    if event.physical == Some(PhysicalKey::KeyL) && state.focus_page() == super::data::Page::Mode {
        state.expand_discovery(false);
        return true;
    }
    if matches!(
        state.focus_page(),
        super::data::Page::Games | super::data::Page::Candidates
    ) {
        if event.physical == Some(PhysicalKey::KeyL) {
            state.select(state.selected().saturating_sub(4));
            return true;
        }
        if event.physical == Some(PhysicalKey::KeyR) {
            state.select(state.selected().saturating_add(4));
            return true;
        }
        if code == KEY_X && state.focus_page() == super::data::Page::Games {
            state.toggle_game_favorite();
            return true;
        }
    }
    if event.physical == Some(PhysicalKey::ArrowUp) {
        if state.focus_page() == super::data::Page::Mode && state.snapshot().lobby {
            state.expand_discovery(false);
        } else {
            state.select(state.selected().saturating_sub(1));
        }
        return true;
    }
    if event.physical == Some(PhysicalKey::ArrowDown) {
        if state.focus_page() == super::data::Page::Lobby
            && state.selected() + 1 >= state.snapshot().rooms.len()
        {
            state.expand_discovery(true);
        } else {
            state.select(state.selected().saturating_add(1));
        }
        return true;
    }
    match code {
        KEY_LEFT => state.select(state.selected().saturating_sub(1)),
        KEY_RIGHT => state.select(state.selected().saturating_add(1)),
        KEY_A | KEY_RETURN => state.confirm(),
        KEY_ESCAPE | 0x62 => state.back_page(),
        KEY_X => state.refresh_rooms(),
        _ => return false,
    }
    true
}
