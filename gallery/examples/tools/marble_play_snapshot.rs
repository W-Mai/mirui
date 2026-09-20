mod lab_snapshot_support;

use mirui::gallery::demos::marble_play;

fn main() {
    lab_snapshot_support::run("marble-play.png", marble_play::VIEWPORT, |app, root| {
        marble_play::setup_app(app, root);
    });
}
