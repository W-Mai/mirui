use mirui::core::reactive::Signal;
use mirui::ui;

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let root = app.spawn_root().id();
    let signal = Signal::new(true);
    let condition = signal.clone();
    let inner = signal.clone();

    app.compose(root, |cx| {
        ui! {
            View () {
                if $condition {
                    if $inner {
                        View ()
                    }
                }
            }
        }
    });
}
