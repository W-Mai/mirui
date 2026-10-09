use mirui::prelude::*;
use mirui::ui::UiScope;

fn main() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    let _ = UiScope::new(&mut app.world, root);
}
