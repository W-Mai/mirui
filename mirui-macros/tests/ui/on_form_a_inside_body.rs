use mirui::ui;

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            View () {
                on Tap { todo!() }
            }
        }
    });
}
