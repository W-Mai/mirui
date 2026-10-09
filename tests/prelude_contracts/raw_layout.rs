use mirui::ui::layout_binding;
use mirui::ui::{LayoutBinding, SharedLayoutBinding};

fn main() {
    let mut app = mirui::prelude::App::headless(100, 100);
    let root = app.spawn_root().id();
    app.compose(root, |tree| {
        tree.bind_layout(root, &[], |_, _, _| {});
    });
}
