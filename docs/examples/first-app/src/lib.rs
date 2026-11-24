use mirui::app::plugins::StdInstantClockPlugin;
use mirui::core::persistence::PersistencePlugin;
use mirui::core::storage::StorageHandle;
use mirui::prelude::*;
use mirui::ui::widgets::{Text, TextDocument};

mod interface;
pub mod model;
mod motion;
#[cfg(target_arch = "wasm32")]
mod web;

pub use model::{Tasks, TasksHandle};

#[derive(Clone, Copy)]
pub struct Chapter(u8);

impl Chapter {
    pub const FINAL: Self = Self(7);

    pub fn new(chapter: u8) -> Option<Self> {
        (1..=Self::FINAL.0)
            .contains(&chapter)
            .then_some(Self(chapter))
    }

    pub fn has_list(self) -> bool {
        self.0 >= 3
    }

    pub fn has_presets(self) -> bool {
        self.0 >= 4
    }

    pub fn has_motion(self) -> bool {
        self.0 >= 5
    }

    pub fn has_storage(self) -> bool {
        self.0 >= 6
    }
}

pub fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    chapter: Chapter,
    reduced: bool,
    storage: Option<StorageHandle>,
) -> Option<TasksHandle> {
    app.with_default_widgets().with_default_systems();
    app.add_plugin(StdInstantClockPlugin);
    let root = app.spawn_root().id();
    if chapter.0 == 1 {
        app.compose(root, first_window());
        return None;
    }

    // #region persistence
    if chapter.has_storage()
        && let Some(storage) = storage
    {
        app.add_plugin(PersistencePlugin::new(storage).autosave(500));
    }
    // #endregion persistence
    if chapter.has_presets() {
        app.add_plugin(
            StyleSheetPlugin::new()
                .class("task", style!(height: 42, border_radius: 10))
                .class(
                    "task.complete",
                    style!(normal_color: ColorToken::Secondary, text_color: ColorToken::OnSecondary),
                )
                .class("filter", style!(height: 36, border_radius: 6)),
        );
    }
    let tasks = app.add_model(Tasks::default()).finish();
    if chapter.has_motion() {
        app.add_resource(motion::MotionSettings(motion::progress_transition(reduced)));
        app.with_widget(motion::paint_progress::view().with_systems([
            motion::aim_progress::system(tasks.clone()),
            motion::AnimateProgress::system(),
        ]));
    }
    let draft = app.add_model(TextDocument::new()).finish();
    app.compose(
        root,
        interface::task_list(tasks.clone(), draft, chapter, reduced),
    );
    Some(tasks)
}

// #region window
#[compose]
fn first_window() {
    ui! {
        Column (padding: Padding::all(22), row_gap: 12) {
            Text ("Task list", font_size: 28, height: 38)
            Text ("Connect the display", height: 26)
            Text ("Open a window", height: 26)
        }
    };
}
// #endregion window
