mod lab_snapshot_support;

use mirui::gallery::demos::pixel_loom;

fn main() {
    lab_snapshot_support::run("pixel-loom.png", pixel_loom::VIEWPORT, |app, root| {
        pixel_loom::setup_app(app, root);
    });
}
