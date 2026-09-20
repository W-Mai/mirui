mod lab_snapshot_support;

use mirui::gallery::demos::lumen_lab;

fn main() {
    lab_snapshot_support::run("lumen-lab.png", lumen_lab::VIEWPORT, |app, root| {
        lumen_lab::setup_app(app, root);
    });
}
