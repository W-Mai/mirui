use mirui::app::App;
use mirui::ui;
use mirui::ui::widgets::Button;

fn main() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            Button (42)
        }
    });

    let _ = Button::default();
}
