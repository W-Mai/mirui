use mirui::prelude::*;
use mirui::ui::widgets::Text;

ui!(compose PlainCard(text: String) {
    Text(text)
});

fn main() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            PlainCard (text: ${ String::from("dynamic") })
        }
    });
}
