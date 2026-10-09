use mirui::app::App;
use mirui::ui;

fn main() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            slot (id: 42)
        }
    });
}
