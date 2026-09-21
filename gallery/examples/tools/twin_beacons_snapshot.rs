mod lab_snapshot_support;

use mirui::gallery::demos::twin_beacons;

fn main() {
    lab_snapshot_support::run("twin-beacons.png", twin_beacons::VIEWPORT, |app, root| {
        twin_beacons::setup_app(app, root);
    });
}
