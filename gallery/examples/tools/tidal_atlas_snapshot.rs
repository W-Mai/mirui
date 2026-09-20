mod lab_snapshot_support;

use mirui::gallery::demos::tidal_atlas;

fn main() {
    lab_snapshot_support::run("tidal-atlas.png", tidal_atlas::VIEWPORT, |app, root| {
        tidal_atlas::setup_app(app, root);
    });
}
