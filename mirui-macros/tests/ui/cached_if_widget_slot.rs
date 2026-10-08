use mirui::core::reactive::Signal;
use mirui::ui;

ui!(compose Card {
    @@body
});

fn main() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_widget(ui!(compose Card));
    let root = app.spawn_root().id();
    let signal = Signal::new(true);
    let condition = signal.clone();

    app.compose(root, |cx| {
        ui! {
            View () {
                if $condition {
                    Card () {
                        @body {
                            View ()
                        }
                    }
                } else {
                    Card () {
                        @body {
                            View ()
                        }
                    }
                }
            }
        }
    });
}
