mod lab_snapshot_support;

use mirui::gallery::demos::layout_lab;

fn main() {
    lab_snapshot_support::run("layout-lab.png", layout_lab::VIEWPORT, |app, root| {
        layout_lab::setup_app(app, root);
    });
}
