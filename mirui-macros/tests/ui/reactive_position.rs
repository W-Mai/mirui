use mirui::core::reactive::Signal;
use mirui::types::Dimension;
use mirui::ui;

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let root = app.spawn_root().id();
    let left = Signal::new(12_i32);
    let top = Signal::new(Dimension::percent(25));

    app.compose(root, |cx| {
        ui! {
            View (
                left: ${ left.get() },
                top: ${ top.get() }
            )
        }
    });
}
