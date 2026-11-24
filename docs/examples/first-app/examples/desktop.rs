use mirui::core::storage::{FileStorage, Storage};
use mirui::prelude::App;
use mirui::surface::sdl::SdlSurface;
use mirui_first_app::{Chapter, install};

fn main() {
    let chapter = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .and_then(Chapter::new)
        .unwrap_or(Chapter::FINAL);
    let storage = if chapter.has_storage() {
        Some(
            FileStorage::open("task-list.mirui")
                .expect("open task storage")
                .into_handle(),
        )
    } else {
        None
    };
    let mut app = App::new(SdlSurface::new("Task list", 420, 440));
    install(&mut app, chapter, false, storage);
    app.run();
}
