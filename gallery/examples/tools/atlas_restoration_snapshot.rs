mod lab_snapshot_support;

use mirui::gallery::demos::atlas_restoration;

fn main() {
    lab_snapshot_support::run(
        "atlas-restoration.png",
        atlas_restoration::VIEWPORT,
        |app, root| {
            atlas_restoration::setup_app(app, root);
        },
    );
}
