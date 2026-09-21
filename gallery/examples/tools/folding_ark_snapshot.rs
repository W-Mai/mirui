mod lab_snapshot_support;

use mirui::gallery::demos::folding_ark;

fn main() {
    lab_snapshot_support::run("folding-ark.png", folding_ark::VIEWPORT, |app, root| {
        folding_ark::setup_app(app, root);
    });
}
