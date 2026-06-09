fn main() {
    gallery::run("mirui - Tween vs Spring vs Elastic", 400, 300, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::spatial_anim::setup_app(setup.app, parent);
        parent
    });
}
