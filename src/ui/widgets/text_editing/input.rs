use core::ops::Range;

use crate::core::model::ModelHandle;
use crate::ecs::{CommandError, Entity, World};
use crate::input::event::focus::{FocusState, Focusable};
use crate::input::event::input::{
    KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT, Key,
    TextCommitted,
};

use super::{Changed, SelectionChanged, Submitted, TextEditError, TextEditing, TextSelection};

#[crate::event]
pub(super) fn preedit(
    entity: entity!(),
    _event: event!(crate::ui::text_input::CompositionUpdated),
    commands: commands!(),
) {
    commands.apply_event::<crate::ui::text_input::CompositionUpdated>(
        entity,
        |world, entity, event| {
            if let Some(editor) = world.get_mut::<TextEditing>(entity)
                && let Err(error) = editor.update_preedit(event.text(), event.selection())
            {
                crate::warn!(target: "mirui::input", "preedit rejected: {:?}", error);
            }
            Ok(())
        },
    );
}

#[crate::event]
pub(super) fn composition_ended(
    entity: entity!(),
    _event: event!(crate::ui::text_input::CompositionEnded),
    editors: query!(mut TextEditing),
) {
    if let Some(editor) = editors.get(entity) {
        editor.clear_preedit();
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Navigation {
    Left,
    Right,
    Home,
    End,
}

impl Navigation {
    pub(crate) fn selection(
        self,
        text: &str,
        selection: TextSelection,
        extend: bool,
    ) -> TextSelection {
        let collapse = !extend && selection.anchor != selection.cursor;
        let cursor = match self {
            Self::Left if collapse => selection.range().start,
            Self::Right if collapse => selection.range().end,
            Self::Left => text[..selection.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(offset, _)| offset),
            Self::Right => {
                selection.cursor
                    + text[selection.cursor..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)
            }
            Self::Home => 0,
            Self::End => text.len(),
        };
        TextSelection {
            anchor: if extend { selection.anchor } else { cursor },
            cursor,
        }
    }
}

#[derive(Clone, Copy)]
enum Edit<'a> {
    Replace(&'a str),
    Backspace,
    Delete,
    Navigate(Navigation, bool),
    Select(TextSelection),
    Normalize,
    Submit,
}

struct Plan<'a> {
    replacement: Option<(Range<usize>, &'a str)>,
    selection: TextSelection,
    content_changed: bool,
}

impl<'a> Edit<'a> {
    fn plan(self, input: &TextEditing) -> Result<Plan<'a>, TextEditError> {
        input.read_document(|document| {
            document.read(|text| {
                let selection = input.selection.normalized(text);
                let mut range = selection.range();
                let replacement = match self {
                    Self::Replace(value) => {
                        if value.chars().any(char::is_control) {
                            return Err(TextEditError::ControlCharacter);
                        }
                        Some(value)
                    }
                    Self::Backspace => {
                        if range.is_empty() {
                            range.start = Navigation::Left.selection(text, selection, false).cursor;
                        }
                        Some("")
                    }
                    Self::Delete => {
                        if range.is_empty() {
                            range.end = Navigation::Right.selection(text, selection, false).cursor;
                        }
                        Some("")
                    }
                    Self::Navigate(direction, extend) => {
                        return Ok(Plan {
                            replacement: None,
                            selection: direction.selection(text, selection, extend),
                            content_changed: false,
                        });
                    }
                    Self::Select(selection) => {
                        for offset in [selection.anchor, selection.cursor] {
                            if !text.is_char_boundary(offset) {
                                return Err(TextEditError::InvalidBoundary { offset });
                            }
                        }
                        return Ok(Plan {
                            replacement: None,
                            selection,
                            content_changed: false,
                        });
                    }
                    Self::Normalize | Self::Submit => None,
                };
                let Some(replacement) = replacement else {
                    return Ok(Plan {
                        replacement: None,
                        selection,
                        content_changed: false,
                    });
                };
                let required = (text.len() - range.len()).saturating_add(replacement.len());
                if required > document.capacity() {
                    return Err(TextEditError::Capacity {
                        required,
                        capacity: document.capacity(),
                    });
                }
                let content_changed = &text[range.clone()] != replacement;
                if content_changed && document.revision() == u64::MAX {
                    return Err(TextEditError::RevisionExhausted);
                }
                let cursor = range.start + replacement.len();
                Ok(Plan {
                    replacement: Some((range, replacement)),
                    selection: TextSelection {
                        anchor: cursor,
                        cursor,
                    },
                    content_changed,
                })
            })
        })
    }
}

#[crate::event]
pub(super) fn attach(
    entity: entity!(),
    editors: query!(mut TextEditing, optional Focusable, optional crate::input::event::scroll::TouchAction),
    commands: commands!(),
) {
    let Some((editor, focusable, touch_action)) = editors.get(entity) else {
        return;
    };
    if focusable.is_none() {
        commands.insert(entity, Focusable);
    }
    if touch_action.is_none() {
        commands.insert(entity, crate::input::event::scroll::TouchAction::PanY);
    }
    commands.insert(entity, crate::ui::HitTarget);
    commands.insert(entity, crate::ui::InteractionFeedback);
    if let Err(error) = editor.prepare_preedit() {
        crate::warn!("preedit preparation failed: {:?}", error);
    }
}

#[crate::event]
pub(super) fn tap(
    node: entity!(),
    event: event!(crate::input::event::gesture::Tap),
    commands: commands!(),
) {
    commands.apply(
        node,
        crate::types::Point {
            x: event.x,
            y: event.y,
        },
        |world, node, point| {
            let Some(cursor) = super::geometry::hit_caret(world, node, point) else {
                return Ok(());
            };
            apply(
                world,
                node,
                Edit::Select(TextSelection {
                    anchor: cursor,
                    cursor,
                }),
            )
        },
    );
}

#[crate::event]
pub(super) fn committed(node: entity!(), _event: event!(TextCommitted), commands: commands!()) {
    commands.apply_event(node, apply_committed);
}

#[crate::event]
pub(super) fn drag_start(
    node: entity!(),
    event: event!(crate::input::event::gesture::DragStart),
    commands: commands!(),
) -> bool {
    commands.apply(
        node,
        crate::types::Point::new(event.x, event.y),
        |world, node, point| {
            if let Some(editor) = world.get_mut::<TextEditing>(node) {
                editor.drag_revision = None;
            }
            let Some(cursor) = super::geometry::hit_caret(world, node, point) else {
                return Ok(());
            };
            let Some(focus) = world.resource_mut::<FocusState>() else {
                return Ok(());
            };
            focus.focused = Some(node);
            let result = apply(
                world,
                node,
                Edit::Select(TextSelection {
                    anchor: cursor,
                    cursor,
                }),
            );
            if let Some(editor) = world.get_mut::<TextEditing>(node)
                && editor.selection()
                    == (TextSelection {
                        anchor: cursor,
                        cursor,
                    })
            {
                editor.drag_revision = Some(super::edit_target::EditVersion::capture(editor));
            }
            result
        },
    );
    true
}

#[crate::event]
pub(super) fn drag_move(
    node: entity!(),
    event: event!(crate::input::event::gesture::DragMove),
    commands: commands!(),
) -> bool {
    commands.apply(
        node,
        (crate::types::Point::new(event.x, event.y), false),
        drag_select,
    );
    true
}

#[crate::event]
pub(super) fn drag_end(
    node: entity!(),
    event: event!(crate::input::event::gesture::DragEnd),
    commands: commands!(),
) -> bool {
    commands.apply(
        node,
        (crate::types::Point::new(event.x, event.y), true),
        drag_select,
    );
    true
}

#[crate::event]
pub(super) fn drag_cancel(
    node: entity!(),
    _event: event!(crate::input::event::gesture::DragCancel),
    editors: query!(mut TextEditing),
) -> bool {
    if let Some(editor) = editors.get(node) {
        editor.drag_revision = None;
    }
    true
}

fn drag_select(
    world: &mut World,
    node: Entity,
    (point, end): (crate::types::Point, bool),
) -> Result<(), CommandError> {
    if !world
        .get::<TextEditing>(node)
        .is_some_and(|editor| editor.usable(world))
    {
        return Ok(());
    }
    let focused = world
        .resource::<FocusState>()
        .is_some_and(|focus| focus.focused == Some(node));
    let Some(editor) = world.get_mut::<TextEditing>(node) else {
        return Ok(());
    };
    let current = super::edit_target::EditVersion::capture(editor);
    if !focused || editor.drag_revision != Some(current) {
        editor.drag_revision = None;
        return Ok(());
    }
    let anchor = editor.selection().anchor;
    if end {
        editor.drag_revision = None;
    }
    let Some(cursor) = super::geometry::hit_caret(world, node, point) else {
        return Ok(());
    };
    let result = apply(world, node, Edit::Select(TextSelection { anchor, cursor }));
    if !end && let Some(editor) = world.get_mut::<TextEditing>(node) {
        editor.drag_revision = Some(super::edit_target::EditVersion::capture(editor));
    }
    result
}

fn apply_committed(
    world: &mut World,
    node: Entity,
    event: &TextCommitted,
) -> Result<(), CommandError> {
    if !crate::ui::text_input::accepts_commit(world, node) {
        return Ok(());
    }
    apply(world, node, Edit::Replace(event.text()))
}

#[crate::event]
pub(super) fn select_all(
    node: entity!(),
    _event: event!(crate::ui::selection::SelectAll),
    editors: query!(mut TextEditing),
    commands: commands!(),
) -> Result<(), crate::ui::selection::RequestError> {
    use crate::ui::selection::RequestError;
    let editor = editors.get(node).ok_or(RequestError::Unavailable)?;
    if !editor.usable_by(crate::core::reactive::current_world_id()) {
        return Err(RequestError::Unavailable);
    }
    let selection = TextSelection {
        anchor: 0,
        cursor: editor.len(),
    };
    if editor
        .set_selection(selection)
        .map_err(|_| RequestError::Rejected)?
    {
        commands.emit(
            node,
            SelectionChanged {
                anchor: 0,
                cursor: selection.cursor as u32,
            },
        );
    }
    Ok(())
}

#[crate::event]
pub(super) fn key(node: entity!(), event: event!(Key), commands: commands!()) -> bool {
    if !event.pressed {
        return false;
    }
    let extend = event.modifiers.is_some_and(|modifiers| modifiers.shift);
    let edit = match event.code {
        KEY_RETURN => Edit::Submit,
        KEY_BACKSPACE => Edit::Backspace,
        KEY_DELETE => Edit::Delete,
        KEY_LEFT => Edit::Navigate(Navigation::Left, extend),
        KEY_RIGHT => Edit::Navigate(Navigation::Right, extend),
        KEY_HOME => Edit::Navigate(Navigation::Home, extend),
        KEY_END => Edit::Navigate(Navigation::End, extend),
        _ => return false,
    };
    commands.apply(node, edit, apply);
    true
}

fn apply(world: &mut World, node: Entity, edit: Edit<'_>) -> Result<(), CommandError> {
    match apply_checked(world, node, edit) {
        Ok(EditOutcome::Complete) => Ok(()),
        Ok(EditOutcome::NotificationFailed(error)) => Err(error),
        Err(error) => {
            crate::warn!(target: "mirui::text_editing", "text edit rejected: {:?}", error);
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditFailure {
    Unavailable,
    Rejected(TextEditError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditOutcome {
    Complete,
    NotificationFailed(CommandError),
}

impl From<TextEditError> for EditFailure {
    fn from(error: TextEditError) -> Self {
        Self::Rejected(error)
    }
}

fn apply_checked(
    world: &mut World,
    node: Entity,
    edit: Edit<'_>,
) -> Result<EditOutcome, EditFailure> {
    match prepare_edit(world, node, edit)? {
        Some(edit) => commit_edit(world, node, edit),
        None => Ok(EditOutcome::Complete),
    }
}

struct PreparedEdit<'a> {
    plan: Plan<'a>,
    target: super::edit_target::EditTarget,
    selection_changed: bool,
    selection_revision: u64,
    submit: bool,
}

fn prepare_edit<'a>(
    world: &World,
    node: Entity,
    edit: Edit<'a>,
) -> Result<Option<PreparedEdit<'a>>, EditFailure> {
    let Some(input) = world
        .get::<TextEditing>(node)
        .filter(|input| input.usable(world))
    else {
        return Err(EditFailure::Unavailable);
    };
    let plan = edit.plan(input)?;
    let selection_changed = input.selection != plan.selection;
    let selection_revision = input
        .selection_revision
        .checked_add(u64::from(selection_changed))
        .ok_or(TextEditError::RevisionExhausted)?;
    if !selection_changed && !plan.content_changed && !matches!(edit, Edit::Submit) {
        return Ok(None);
    }
    let Some(target) = super::edit_target::EditTarget::capture(world, node) else {
        return Err(EditFailure::Unavailable);
    };
    Ok(Some(PreparedEdit {
        plan,
        target,
        selection_changed,
        selection_revision,
        submit: matches!(edit, Edit::Submit),
    }))
}

fn commit_edit(
    world: &mut World,
    node: Entity,
    edit: PreparedEdit<'_>,
) -> Result<EditOutcome, EditFailure> {
    let PreparedEdit {
        plan,
        target,
        selection_changed,
        selection_revision,
        submit,
    } = edit;
    if plan.content_changed {
        let (range, text) = plan
            .replacement
            .as_ref()
            .expect("content edit has a replacement");
        let result = match &target.identity.document {
            Some(document) => document.replace(range.clone(), text),
            None => world
                .get_mut::<TextEditing>(node)
                .unwrap()
                .local
                .as_mut()
                .unwrap()
                .replace(range.clone(), text),
        };
        result?;
    }
    let Some(input) = target.after_edit(world, plan.content_changed) else {
        return Err(EditFailure::Unavailable);
    };
    let len = input.len() as u32;
    if selection_changed {
        let input = world.get_mut::<TextEditing>(node).unwrap();
        input.selection = plan.selection;
        input.selection_revision = selection_revision;
    }
    let content_result = if plan.content_changed {
        crate::input::event::emission::submit_if_listened(world, node, Changed { len })
    } else {
        Ok(())
    };
    let selection_result = if selection_changed {
        crate::input::event::emission::submit_if_listened(
            world,
            node,
            SelectionChanged {
                anchor: plan.selection.anchor as u32,
                cursor: plan.selection.cursor as u32,
            },
        )
    } else {
        Ok(())
    };
    let submit_result = if submit {
        crate::input::event::emission::submit_if_listened(world, node, Submitted)
    } else {
        Ok(())
    };
    Ok(
        match content_result.and(selection_result).and(submit_result) {
            Ok(()) => EditOutcome::Complete,
            Err(error) => EditOutcome::NotificationFailed(error),
        },
    )
}

pub(super) fn validate_replacement(editor: &TextEditing, text: &str) -> Result<(), TextEditError> {
    let plan = Edit::Replace(text).plan(editor)?;
    editor
        .selection_revision
        .checked_add(u64::from(editor.selection != plan.selection))
        .ok_or(TextEditError::RevisionExhausted)?;
    Ok(())
}

pub(super) fn replace_from_clipboard(
    world: &mut World,
    node: Entity,
    text: &str,
) -> Result<(), CommandError> {
    let rejected = |error| {
        crate::warn!(target: "mirui::text_editing", "clipboard edit rejected: {:?}", error);
        CommandError::Commit(crate::ecs::CommandCommitError::OperationRejected {
            operation: "text replacement",
        })
    };
    let Some(edit) = prepare_edit(world, node, Edit::Replace(text)).map_err(rejected)? else {
        return Ok(());
    };
    let interrupted = crate::ui::text_input::interrupt_edit(world, node);
    let result = commit_edit(world, node, edit);
    interrupted.finish(world);
    match result.map_err(rejected)? {
        EditOutcome::Complete => {}
        EditOutcome::NotificationFailed(error) => {
            crate::warn!(target: "mirui::text_editing", "clipboard edit notification skipped: {:?}", error);
        }
    }
    Ok(())
}

#[crate::system(order = PreRender)]
pub(super) fn normalize_selections(editors: query!(Entity, TextEditing), commands: commands!()) {
    for (entity, editor) in editors {
        if editor
            .document
            .as_ref()
            .is_some_and(|document| !document.callback_ready())
        {
            continue;
        }
        if editor.selection != editor.selection() {
            commands.apply(entity, Edit::Normalize, apply);
        }
    }
}

#[crate::system(order = PreRender)]
pub(super) fn cursor_blink(
    clock: res!(optional crate::ecs::MonoClock),
    phase: res!(optional super::CursorBlinkPhase),
    focus: res!(optional FocusState),
    editors: query!(Entity, TextEditing),
    commands: commands!(),
) {
    let next_phase = clock.is_none_or(|clock| ((clock.clock)() / 500_000_000) % 2 == 0);
    let phase_changed = phase.is_none_or(|phase| phase.0 != next_phase);
    if phase_changed {
        commands.insert_resource(super::CursorBlinkPhase(next_phase));
    }
    let focused = focus.and_then(|focus| focus.focused);
    for (entity, editor) in editors {
        let now = focused == Some(entity);
        if editor.focused != now {
            commands.apply(entity, now, |world, entity, focused| {
                if let Some(editor) = world.get_mut::<TextEditing>(entity) {
                    editor.focused = focused;
                }
                Ok(())
            });
        } else if now && phase_changed {
            commands.insert(entity, crate::ui::dirty::VisualDirty::New);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod event_tests;

#[cfg(test)]
mod queue_tests;

#[cfg(test)]
mod drag_tests;
