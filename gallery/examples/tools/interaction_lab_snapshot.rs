mod lab_snapshot_support;

use mirui::gallery::demos::interaction_lab;

fn main() {
    lab_snapshot_support::run(
        "interaction-lab.png",
        interaction_lab::VIEWPORT,
        |app, root| {
            interaction_lab::setup_app(app, root);
        },
    );
}
