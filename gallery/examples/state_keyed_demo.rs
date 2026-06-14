fn main() {
    gallery::run("mirui - keyed walk reorder", 320, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::state_keyed::setup_app(setup.app, parent);
        parent
    });
}
