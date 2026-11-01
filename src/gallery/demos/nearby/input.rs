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

fn contains(root: Entity, mut node: Entity, parent: impl Fn(Entity) -> Option<Entity>) -> bool {
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
    if code == 0x79 {
        library.adjust_tool(super::launcher::Tool::Theme);
        return true;
    }
    if library.active() {
        let direction = match event.physical {
            Some(PhysicalKey::ArrowLeft | PhysicalKey::ArrowUp) => Some(false),
            Some(PhysicalKey::ArrowRight | PhysicalKey::ArrowDown) => Some(true),
            _ => None,
        };
        if let Some(forward) = direction {
            super::launcher_view::move_selection(state, forward);
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
