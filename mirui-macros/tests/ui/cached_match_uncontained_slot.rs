use mirui::core::reactive::Signal;
use mirui::ui;

ui!(compose Card {
    @@body
});

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    let root = app.spawn_root().id();
    let signal = Signal::new(true);
    let selection = signal.clone();

    app.compose(root, |cx| {
        ui! {
            Card () {
                match $selection {
                    true => {
                        @body {
                            View ()
                        }
                    }
                    false => {
                        @body {
                            View ()
                        }
                    }
                }
            }
        }
    });
}
