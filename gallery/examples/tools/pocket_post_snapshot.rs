mod lab_snapshot_support;

use mirui::gallery::demos::pocket_post;

fn main() {
    lab_snapshot_support::run("pocket-post.png", pocket_post::VIEWPORT, |app, root| {
        pocket_post::setup_app(app, root);
    });
}
