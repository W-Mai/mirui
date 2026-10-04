use mirui::compose;
use mirui::ui;

#[compose]
fn panel(count: f32) {
    ui! {
        View () on Tap(count) { }
    };
}

fn main() {}
