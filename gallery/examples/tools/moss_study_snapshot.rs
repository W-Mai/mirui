mod lab_snapshot_support;

use mirui::gallery::demos::moss_study;

fn main() {
    lab_snapshot_support::run("moss-study.png", moss_study::VIEWPORT, |app, root| {
        moss_study::setup_app(app, root);
    });
}
