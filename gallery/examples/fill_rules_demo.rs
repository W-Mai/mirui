fn main() {
    gallery::run("mirui - fill rules", 400, 240, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::fill_rules::setup_app(setup.app, parent);
        parent
    });
}
