use mirui::prelude::*;

#[compose]
fn panel() {
    let _ = cx.world_mut();
}

fn main() {
    let mut app = App::headless(100, 100);
    let root = app.spawn_root().id();
    app.compose(root, |tree| {
        let _ = tree.world_mut();
    });
}
