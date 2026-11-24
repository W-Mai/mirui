use super::Chapter;
use super::model::{Filter, Task, Tasks};
use super::motion::{Progress, ProgressTarget, indicator_transition, progress_ratio};
use mirui::core::model::{ModelHandle, ModelItemId};
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::ui::widgets::{
    Button, FollowBounds, ParagraphStyle, Placeholder, Text, TextDocument, TextEditing, TextInput,
};

#[derive(Clone, Copy)]
enum Edit {
    Add,
    Remove(ModelItemId),
}

#[event]
fn edit_tasks(edit: Edit, tasks: model!(Tasks), draft: model!(TextDocument)) {
    match edit {
        Edit::Add => {
            let title = draft.read(|text| {
                let value = text.trim();
                (!value.is_empty()).then(|| value.chars().take(40).collect::<String>())
            });
            tasks.add(title);
            let _ = draft.set_text("");
        }
        Edit::Remove(key) => tasks.remove(key),
    }
}

#[event]
fn select_filter(
    filter: Filter,
    tasks: model!(Tasks),
    selected: entity!(),
    marker: Option<Entity>,
    followers: query!(mut FollowBounds),
) {
    tasks.set_filter(filter);
    if let Some(marker) = marker.and_then(|marker| followers.get(marker)) {
        marker.target = selected;
    }
}

#[compose]
fn task_row(
    tasks: model!(Tasks),
    task: model!(Task),
    draft: model!(TextDocument),
    key: ModelItemId,
    chapter: Chapter,
) -> Entity {
    let title = task.read(|task| task.title.clone());
    ui! {
        Row (
            visible: ${ tasks.filter().includes(task.done()) },
            height: 42,
            column_gap: 6
        ) {
            Button (
                text: ${ format_args!("{}  {}", if task.done() { "[x]" } else { "[ ]" }, title) },
                grow: 1.0,
                min_width: 0,
                height: 42,
                clip_children: true,
                capacity: 172,
                class: ${
                    (
                        chapter.has_presets().then_some("task"),
                        (chapter.has_presets() && task.done()).then_some("task.complete"),
                    )
                }
            ) on Tap { task.toggle(); }
            if chapter.has_list() {
                Button ("Remove", width: 70, height: 42, font_size: 12) on Tap(edit_tasks::callback(Edit::Remove(key), tasks, draft))
            }
        }
    }
}

// #region filter-button
#[compose]
fn filter_button(
    tasks: model!(Tasks),
    title: prop!(String),
    filter: Filter,
    marker: Option<Entity>,
    chapter: Chapter,
) -> Entity {
    ui! {
        Button (
            text: ${ title },
            grow: 1.0,
            height: 36,
            class: chapter.has_presets().then_some("filter")
        ) on Tap(select_filter::callback(filter, tasks, marker))
    }
}
// #endregion filter-button

#[compose]
fn all_tasks(tasks: model!(Tasks), marker: Option<Entity>, chapter: Chapter) -> Entity {
    ui! {
        Button (
            "All",
            id: "filter_all",
            grow: 1.0,
            height: 36,
            class: chapter.has_presets().then_some("filter")
        ) on Tap(select_filter::callback(Filter::All, tasks, marker))
    }
}

#[compose]
fn task_controls(
    tasks: model!(Tasks),
    draft: model!(TextDocument),
    chapter: Chapter,
    reduced: bool,
) -> Entity {
    let (open, done) = (Filter::Open, Filter::Done);
    ui! {
        Column (height: 85, row_gap: 9, shrink: 0.0) {
            Row (height: 38, column_gap: 6) {
                all_tasks (tasks, chapter.has_motion().then(|| id("filter_marker")), chapter)
                filter_button (
                    tasks,
                    "Open",
                    open,
                    chapter.has_motion().then(|| id("filter_marker")),
                    chapter
                )
                filter_button (
                    tasks,
                    "Done",
                    done,
                    chapter.has_motion().then(|| id("filter_marker")),
                    chapter
                )
                View (
                    id: "filter_marker",
                    visible: ${ chapter.has_motion() },
                    border_color: ColorToken::Primary,
                    border_width: 2,
                    border_radius: 6
                ) [
                    FollowBounds::new(id("filter_all"))
                        .with_transition(indicator_transition(reduced || !chapter.has_motion())),
                ]
            }
            Row (height: 38, column_gap: 6) {
                TextInput (
                    grow: 1.0,
                    min_width: 0,
                    height: 38,
                    padding: Padding::all(6),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8
                ) [
                    TextEditing::new(draft.clone()),
                    Placeholder("New task"),
                ] on TextEditing::Submitted(edit_tasks::callback(Edit::Add, tasks, draft))
                Button ("Add", width: 66, height: 38) on Tap(edit_tasks::callback(Edit::Add, tasks, draft))
            }
        }
    }
}

#[compose]
pub(crate) fn task_list(
    tasks: model!(Tasks),
    draft: model!(TextDocument),
    chapter: Chapter,
    reduced: bool,
) {
    let start = progress_ratio(tasks.completed(), tasks.total());
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            padding: Padding::all(18),
            row_gap: 9,
            text_color: ColorToken::OnSurface,
            font_size: 14
        ) {
            Text ("Task list", font_size: 26, height: 35)
            Text (
                text: ${ format_args!("{} / {} complete", tasks.completed(), tasks.total()) },
                capacity: 32,
                paragraph: ParagraphStyle::label(),
                height: 23,
                text_color: ColorToken::OnSurfaceVariant
            )
            if chapter.has_motion() {
                View (height: 6, bg_color: ColorToken::SurfaceVariant, border_radius: 3) [
                    Progress(start),
                    ProgressTarget(start),
                ]
            }
            if chapter.has_list() {
                task_controls (tasks, draft, chapter, reduced)
            }
            Column (grow: 1.0, min_height: 0, row_gap: 5, clip_children: true) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                walk ${ tasks.items().entries() } with row by row.0 {
                    task_row (
                        tasks,
                        row.1,
                        draft,
                        row.0,
                        chapter
                    )
                }
            }
        }
    };
}
