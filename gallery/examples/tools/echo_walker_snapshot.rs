mod lab_snapshot_support;

use mirui::gallery::demos::echo_walker;

fn main() {
    lab_snapshot_support::run("echo-walker.png", echo_walker::VIEWPORT, |app, root| {
        echo_walker::setup_app(app, root);
    });
}
