mod lab_snapshot_support;

use mirui::gallery::demos::curve_text;

fn main() {
    lab_snapshot_support::run("curve-text.png", curve_text::VIEWPORT, |app, root| {
        curve_text::setup_app(app, root);
    });
}
