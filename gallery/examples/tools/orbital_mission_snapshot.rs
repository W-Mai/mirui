mod lab_snapshot_support;

use mirui::gallery::demos::orbital_mission;

fn main() {
    lab_snapshot_support::run(
        "orbital-mission.png",
        orbital_mission::VIEWPORT,
        |app, root| {
            orbital_mission::setup_app(app, root);
        },
    );
}
