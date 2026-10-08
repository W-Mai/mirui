use mirui::ui;

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            View (
                width: @(width, height, id(a).width, id(b).width) { width },
                height: @id(c).height { c.height },
            )
        }
    });
}
