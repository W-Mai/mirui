fn main() {
    gallery::run("vector_mandala_demo", 512, 512, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::vector_mandala::setup_app(setup.app, parent);
        parent
    });
}
