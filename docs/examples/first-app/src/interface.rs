use super::Chapter;
use super::model::{Filter, TASK_LIMIT, Task, Tasks};
use super::motion::{Progress, ProgressTarget, indicator_transition, progress_ratio};
use mirui::core::model::{ModelHandle, ModelItemId};
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::ui::icons::ICON_CHECK;
use mirui::ui::widgets::{
    Button, FollowBounds, Icon, ParagraphStyle, Placeholder, Text, TextAlign, TextDocument,
    TextEditing, TextInput,
};

#[derive(Clone, Copy)]
enum Edit {
    Add,
    Remove(ModelItemId),
}

//~focus-start:ch3
//~add-start:ch3
#[event]
fn edit_tasks(edit: Edit, tasks: model!(Tasks), draft: model!(TextDocument)) {
    match edit {
        Edit::Add => {
            if tasks.total() == TASK_LIMIT {
                return;
            }
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
//~add-end:ch3
//~focus-end:ch3

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

//~focus-start:ch2,ch4
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
        Column (visible: ${ tasks.filter().includes(task.done()) }, height: 49, shrink: 0.0) {
            Row (height: 48, align: AlignItems::Center) {
                //~add-start:ch2
                Button (
                    grow: 1.0,
                    min_width: 0,
                    height: 44,
                    clip_children: true,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::SurfaceVariant,
                    //~add-start:ch4
                    class: ${
                        (
                            chapter.has_presets().then_some("task"),
                            (chapter.has_presets() && task.done()).then_some("task.complete"),
                        )
                    }
                    //~add-end:ch4
                ) {
                    Row (grow: 1.0, min_width: 0, align: AlignItems::Center, column_gap: 12) {
                        Column (
                            width: 20,
                            height: 20,
                            shrink: 0.0,
                            border_width: 1,
                            border_radius: 6,
                            align: AlignItems::Center,
                            justify: JustifyContent::Center,
                            bg_color: ${ if task.done() { ColorToken::Primary } else { ColorToken::Surface } },
                            border_color: ${ if task.done() { ColorToken::Primary } else { ColorToken::OnSurfaceVariant } }
                        ) {
                            Icon (
                                visible: ${ task.done() },
                                path: ICON_CHECK,
                                width: 14,
                                height: 14,
                                color: ColorToken::OnPrimary
                            )
                        }
                        Text (
                            text: title,
                            grow: 1.0,
                            min_width: 0,
                            height: 22,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                    }
                } on Tap { task.toggle(); }
                //~add-end:ch2
                if chapter.has_list() {
                    Button (
                        "Remove",
                        width: 54,
                        height: 44,
                        font_size: 12,
                        normal_color: ColorToken::Surface,
                        pressed_color: ColorToken::SurfaceVariant,
                        text_color: ColorToken::OnSurfaceVariant
                    ) on Tap(edit_tasks::callback(Edit::Remove(key), tasks, draft))
                }
            }
            View (height: 1, bg_color: ColorToken::Outline)
        }
    }
}
//~focus-end:ch2,ch4

fn filter_style() -> StylePreset<Button> {
    style!(
    grow: 1.0, height: 32, border_radius: 7,
    pressed_color: ColorToken::Surface, text_color: ColorToken::OnSurface
    )
}

fn tab_surface(active: bool) -> ColorToken {
    if active {
        ColorToken::Surface
    } else {
        ColorToken::SurfaceVariant
    }
}

// #region filter-button
//~focus-start:ch4
//~add-start:ch4
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
            style: filter_style(),
            normal_color: ${ tab_surface(tasks.filter() == filter) },
            class: chapter.has_presets().then_some("filter")
        ) on Tap(select_filter::callback(filter, tasks, marker))
    }
}
//~add-end:ch4
//~focus-end:ch4
// #endregion filter-button

#[compose]
fn all_tasks(tasks: model!(Tasks), marker: Option<Entity>, chapter: Chapter) -> Entity {
    ui! {
        Button (
            "All",
            id: "filter_all",
            style: filter_style(),
            normal_color: ${ tab_surface(tasks.filter() == Filter::All) },
            class: chapter.has_presets().then_some("filter")
        ) on Tap(select_filter::callback(Filter::All, tasks, marker))
    }
}

//~focus-start:ch5
#[compose]
fn filters(tasks: model!(Tasks), chapter: Chapter, reduced: bool) -> Entity {
    let (open, done) = (Filter::Open, Filter::Done);
    ui! {
        Row (
            height: 38,
            shrink: 0.0,
            padding: Padding::all(3),
            bg_color: ColorToken::SurfaceVariant,
            border_radius: 10
        ) {
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
            //~add-start:ch5
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
            //~add-end:ch5
        }
    }
}
//~focus-end:ch5

//~focus-start:ch3
//~add-start:ch3
#[compose]
fn add_task(tasks: model!(Tasks), draft: model!(TextDocument)) -> Entity {
    ui! {
        Row (height: 42, shrink: 0.0, column_gap: 8) {
            TextInput (
                grow: 1.0,
                min_width: 0,
                height: 42,
                padding: Padding::all(10),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 9
            ) [
                TextEditing::new(draft.clone()),
                Placeholder("New task"),
            ] on TextEditing::Submitted(edit_tasks::callback(Edit::Add, tasks, draft))
            Button (
                text: ${ if tasks.total() == TASK_LIMIT { "List full" } else { "+ Add" } },
                width: 70,
                height: 42,
                border_radius: 9,
                normal_color: ColorToken::OnSurface,
                pressed_color: ColorToken::Primary,
                text_color: ColorToken::Surface
            ) on Tap(edit_tasks::callback(Edit::Add, tasks, draft))
        }
    }
}
//~add-end:ch3
//~focus-end:ch3

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
            //~focus-start:ch2,ch5
            Column (
                height: if chapter.has_motion() { 102 } else { 92 },
                padding: Padding::all(14),
                row_gap: 6,
                shrink: 0.0,
                bg_color: ColorToken::Secondary,
                border_radius: 12
            ) {
                Text ("Task list", font_size: 30, height: 39, text_color: ColorToken::OnSecondary)
                //~add-start:ch2
                Text (
                    text: ${ format_args!("{} of {} complete", tasks.completed(), tasks.total()) },
                    capacity: 32,
                    font_size: 13,
                    height: 19,
                    text_color: ColorToken::OnSecondary
                )
                //~add-end:ch2
                //~add-start:ch5
                if chapter.has_motion() {
                    View (height: 4, bg_color: ColorToken::OnSurfaceVariant, border_radius: 2) [
                        Progress(start),
                        ProgressTarget(start),
                    ]
                }
                //~add-end:ch5
            }
            //~focus-end:ch2,ch5
            if chapter.has_list() {
                filters (tasks, chapter, reduced)
            }
            //~focus-start:ch3
            Column (grow: 1.0, min_height: 0, clip_children: true) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                //~add-start:ch3
                walk ${ tasks.items().entries() } with row by row.0 {
                    task_row (
                        tasks,
                        row.1,
                        draft,
                        row.0,
                        chapter
                    )
                }
                //~add-end:ch3
                if chapter.has_list() {
                    Text (
                        visible: ${
                            match tasks.filter() {
                                Filter::All => tasks.total() == 0,
                                Filter::Open => tasks.completed() == tasks.total(),
                                Filter::Done => tasks.completed() == 0,
                            }
                        },
                        text: ${
                            match tasks.filter() {
                                Filter::All => "Add a task to get started.",
                                Filter::Open => "No open tasks.",
                                Filter::Done => "No completed tasks.",
                            }
                        },
                        height: 44,
                        text_color: ColorToken::OnSurfaceVariant
                    )
                }
            }
            //~focus-end:ch3
            if chapter.has_list() {
                add_task (tasks, draft)
            }
        }
    };
}
