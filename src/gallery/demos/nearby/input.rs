use super::model::{DisplayState, PageKey};
use crate::prelude::*;
use crate::ui::Parent;
use crate::ui::widgets::FollowBounds;

#[component]
pub(super) struct SelectionTarget {
    pub frame: Option<Entity>,
    pub index: usize,
    pub state: model!(DisplayState),
}

#[component]
pub(super) struct SelectionFrame {
    pub body: Entity,
    pub page: PageKey,
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
fn initialize(
    node: entity!(),
    targets: query!(mut SelectionTarget),
    frames: query!(Entity, SelectionFrame),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(target) = targets.get(node) else {
        return;
    };
    for (frame, marker) in frames {
        if contains(marker.body, node, |entity| parents.get(entity).map(|p| p.0)) {
            target.frame = Some(frame);
            if target.state.selected() == target.index {
                commands.insert(
                    frame,
                    FollowBounds::new(node).with_transition(crate::anim::SNAPPY),
                );
            }
            break;
        }
    }
}

#[view(component = SelectionTarget, attach = initialize::callback())]
pub(super) fn selection_target() {}

#[event]
fn initialize_frame(
    node: entity!(),
    frames: query!(SelectionFrame),
    targets: query!(Entity, mut SelectionTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(frame) = frames.get(node) else {
        return;
    };
    for (entity, target) in targets {
        if contains(frame.body, entity, |entity| {
            parents.get(entity).map(|p| p.0)
        }) {
            target.frame = Some(node);
            if target.state.selected() == target.index {
                commands.insert(
                    node,
                    FollowBounds::new(entity).with_transition(crate::anim::SNAPPY),
                );
            }
        }
    }
}

#[view(component = SelectionFrame, attach = initialize_frame::callback())]
pub(super) fn selection_frame() {}

#[event]
pub(super) fn place(
    target: entity!(),
    state: model!(DisplayState),
    index: usize,
    targets: query!(SelectionTarget),
    frames: query!(mut FollowBounds),
) {
    if state.selected() == index
        && let Some(frame) = targets.get(target).and_then(|row| row.frame)
        && let Some(bounds) = frames.get(frame)
    {
        bounds.target = target;
    }
}

#[event]
pub(super) fn hover(
    event: event!(HoverChanged),
    target: entity!(),
    state: model!(DisplayState),
    index: usize,
    targets: query!(SelectionTarget),
    frames: query!(mut FollowBounds),
) {
    if event.hovered {
        state.select(index);
        if let Some(frame) = targets.get(target).and_then(|row| row.frame)
            && let Some(bounds) = frames.get(frame)
        {
            bounds.target = target;
        }
    }
}

#[event]
pub(super) fn keyboard(
    event: event!(Key),
    state: model!(DisplayState),
    targets: query!(Entity, SelectionTarget),
    frames: query!(SelectionFrame, mut FollowBounds),
) -> bool {
    use crate::input::event::input::{
        KEY_A, KEY_ESCAPE, KEY_LEFT, KEY_RETURN, KEY_RIGHT, KEY_X, PhysicalKey,
    };
    if event
        .modifiers
        .is_some_and(|m| m.meta || m.control || m.alt)
    {
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
    if library.active() && code == KEY_X {
        if event.pressed {
            super::launcher_view::begin_play(state);
        } else {
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
            library.close();
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
    match code {
        KEY_LEFT => state.select(state.selected().saturating_sub(1)),
        KEY_RIGHT => state.select(state.selected().saturating_add(1)),
        KEY_A | KEY_RETURN => state.confirm(),
        KEY_ESCAPE | 0x62 => state.back_page(),
        KEY_X => state.refresh_rooms(),
        _ => return false,
    }
    let page = state.page_key();
    let selected = state.selected();
    for (target, row) in targets {
        if row.index == selected
            && let Some(frame) = row.frame
            && let Some((marker, bounds)) = frames.get(frame)
            && marker.page == page
        {
            bounds.target = target;
        }
    }
    true
}
