use crate::{Chapter, Recipe, TasksHandle, install, install_named_recipe};
use mirui::app::App;
use mirui::core::storage::{LocalStorageStorage, Storage};
use mirui::render::web_canvas::WebCanvasRendererFactory;
use mirui::surface::web_canvas::WebCanvasSurface;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

type ModelHost =
    App<mirui::surface::framebuf::FramebufSurface<fn(&[u8], mirui::types::PhysicalRect)>>;

#[wasm_bindgen]
pub struct Tutorial {
    app: App<WebCanvasSurface, WebCanvasRendererFactory>,
}

#[wasm_bindgen]
pub struct FrameExample {
    _app: ModelHost,
    tasks: TasksHandle,
}

#[wasm_bindgen]
impl FrameExample {
    /// Keep the same tutorial Model registered without creating a visible runner.
    pub fn create() -> Self {
        let mut app = App::headless(1, 1);
        let tasks = app.add_model(crate::Tasks::default()).finish();
        Self { _app: app, tasks }
    }

    pub fn toggle(&self) -> bool {
        let task = self.tasks.items().get(0).expect("initial task");
        task.toggle();
        task.done()
    }

    pub fn completed(&self) -> usize {
        self.tasks.completed()
    }
}

#[wasm_bindgen]
impl Tutorial {
    #[wasm_bindgen(constructor)]
    pub fn new(
        canvas: HtmlCanvasElement,
        chapter: u8,
        reduced: bool,
        dark: bool,
        persist: bool,
    ) -> Result<Self, JsError> {
        let chapter = Chapter::new(chapter).ok_or_else(|| JsError::new("Unknown chapter"))?;
        // #region web-host
        let surface = WebCanvasSurface::new(canvas);
        let mut app = App::with_factory(surface, WebCanvasRendererFactory::new());
        app.set_theme(if dark { "dark" } else { "light" })
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        let storage = if persist && chapter.has_storage() {
            LocalStorageStorage::with_prefix(concat!("mirui-first-app-", env!("CARGO_PKG_VERSION")))
                .ok()
                .map(Storage::into_handle)
        } else {
            None
        };
        install(&mut app, chapter, reduced, storage);
        app.prepare_text_layout()
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(Self { app })
        // #endregion web-host
    }

    pub fn from_recipe(
        canvas: HtmlCanvasElement,
        name: &str,
        reduced: bool,
        dark: bool,
    ) -> Result<Self, JsError> {
        let recipe = Recipe::parse(name).ok_or_else(|| JsError::new("Unknown recipe"))?;
        let surface = WebCanvasSurface::new(canvas);
        let mut app = App::with_factory(surface, WebCanvasRendererFactory::new());
        app.set_theme(if dark { "dark" } else { "light" })
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        install_named_recipe(&mut app, recipe, reduced);
        app.prepare_text_layout()
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(Self { app })
    }

    pub fn tick(&mut self) -> bool {
        self.app.tick()
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.app.with_theme(crate::palette::book(dark));
    }
}
