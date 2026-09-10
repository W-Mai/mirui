mod lab_snapshot_support;

use mirui::gallery::demos::typography_lab;

fn main() {
    lab_snapshot_support::run(
        "typography-lab.png",
        typography_lab::VIEWPORT,
        |app, root| {
            typography_lab::setup_app(app, root);
        },
    );
}
