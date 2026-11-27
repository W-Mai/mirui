use mirui::app::plugins::StdInstantClockPlugin;
use mirui::core::cache::MaxSize;
use mirui::core::persistence::PersistencePlugin;
use mirui::core::storage::StorageHandle;
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager};
use mirui::ui::widgets::{Text, TextDocument};

mod interface;
pub mod model;
mod motion;
mod palette;
pub mod recipes;
#[cfg(target_arch = "wasm32")]
mod web;

pub use model::{Tasks, TasksHandle};
pub use recipes::Recipe;

const UI_FONT: &[u8] = include_bytes!("../../../../src/gallery/demos/assets/ui_geist.mirx");

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
    app.with_theme(palette::book(app.theme().id().as_str() == "dark"));
    app.with_default_widgets().with_default_systems();
    let font = Font::from_mirx("Geist", 14, UI_FONT, &Default::default())
        .expect("decode the bundled tutorial font");
    app.with_fonts(FontManager::new(MaxSize::Unbound, font));
    app.add_plugin(StdInstantClockPlugin);
    //~focus-start:ch1
    //~add-start:ch1
    let root = app.spawn_root().id();
    if chapter.0 == 1 {
        app.compose(root, first_window());
        return None;
    }
    //~add-end:ch1
    //~focus-end:ch1

    // #region persistence
    //~focus-start:ch6
    //~add-start:ch6
    if chapter.has_storage()
        && let Some(storage) = storage
    {
        app.add_plugin(PersistencePlugin::new(storage).autosave(500));
    }
    //~add-end:ch6
    //~focus-end:ch6
    // #endregion persistence
    if chapter.has_presets() {
        // #region task-styles
        //~focus-start:ch4
        //~add-start:ch4
        app.add_plugin(
            StyleSheetPlugin::new()
                .class("task", style!(text_color: ColorToken::OnSurface))
                .class(
                    "task.complete",
                    style!(text_color: ColorToken::OnSurfaceVariant),
                )
                .class("filter", style!(font_size: 13)),
        );
        //~add-end:ch4
        //~focus-end:ch4
        // #endregion task-styles
    }
    //~focus-start:ch2,ch3,ch6
    //~add-start:ch2
    let tasks = app.add_model(Tasks::default()).finish();
    //~add-end:ch2
    //~focus-end:ch2,ch3,ch6
    if chapter.has_motion() {
        //~focus-start:ch5
        //~add-start:ch5
        app.add_resource(motion::MotionSettings(motion::progress_transition(reduced)));
        app.with_widget(motion::paint_progress::view().with_systems([
            motion::aim_progress::system(tasks.clone()),
            motion::AnimateProgress::system(),
        ]));
        //~add-end:ch5
        //~focus-end:ch5
    }
    //~focus-start:ch2,ch3
    //~add-start:ch3
    let draft = app.add_model(TextDocument::new()).finish();
    //~add-end:ch3
    //~add-start:ch2
    app.compose(
        root,
        interface::task_list(tasks.clone(), draft, chapter, reduced),
    );
    //~add-end:ch2
    //~focus-end:ch2,ch3
    Some(tasks)
}

pub fn install_named_recipe<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    recipe: Recipe,
    reduced: bool,
    storage: Option<StorageHandle>,
) {
    app.with_theme(palette::book(app.theme().id().as_str() == "dark"));
    app.with_default_widgets().with_default_systems();
    let font = Font::from_mirx("Geist", 14, UI_FONT, &Default::default())
        .expect("decode the bundled tutorial font");
    app.with_fonts(FontManager::new(MaxSize::Unbound, font));
    app.add_plugin(StdInstantClockPlugin);
    recipes::install_recipe(app, recipe, reduced, storage);
}

// #region window
//~focus-start:ch1
//~add-start:ch1
#[compose]
fn first_window() {
    ui! {
        Column (grow: 1.0, padding: Padding::all(20), row_gap: 16) {
            Column (
                height: 76,
                padding: Padding::all(16),
                bg_color: ColorToken::Secondary,
                border_radius: 12
            ) {
                Text ("Task list", font_size: 32, height: 44, text_color: ColorToken::OnSecondary)
            }
            Text ("Connect the display", height: 26)
            Text ("Open a window", height: 26)
        }
    };
}
//~add-end:ch1
//~focus-end:ch1
// #endregion window
